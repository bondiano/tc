use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tc_core::dag::TaskDag;
use tc_core::status::StatusMachine;
use tc_core::task::TaskId;
use tc_executor::log_tail;
use tc_spawn::process::WorkerState;
use tc_spawn::recovery;
use tc_spawn::scheduler::list_worker_states;
use tc_spawn::worktree::WorktreeManager;

use crate::cli::{AttachArgs, KillArgs, LogsArgs, SpawnArgs, WorkersArgs};
use crate::error::CliError;
use crate::output;

pub async fn run(args: SpawnArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let config = store.load_config()?;
    let tasks = store.load_tasks()?;
    let sm = StatusMachine::new(config.statuses.clone());

    // Determine which tasks to spawn
    let task_ids: Vec<TaskId> = if !args.task_ids.is_empty() {
        args.task_ids.iter().map(|id| TaskId(id.clone())).collect()
    } else {
        // Discover ready tasks
        let dag = TaskDag::from_tasks(&tasks)?;
        let ready = dag.compute_ready(&tasks, &sm);

        let filtered: Vec<TaskId> = if let Some(ref epic) = args.epic {
            ready
                .into_iter()
                .filter(|id| {
                    tasks
                        .iter()
                        .find(|t| t.id == *id)
                        .is_some_and(|t| &t.epic == epic)
                })
                .collect()
        } else {
            ready
        };

        if filtered.is_empty() {
            return Err(CliError::user("no ready tasks to spawn"));
        }

        filtered
    };

    let max_parallel = args.max.unwrap_or(config.spawn.max_parallel);
    let worktree_mgr = WorktreeManager::new(store.root().clone(), config.spawn.clone());

    let executor = match config.executor.default {
        tc_core::config::ExecutorKind::Opencode => build_executor_opencode(),
        // spawn only ships claude/opencode backends; custom ones fall back
        // to claude so headless runs still work from a TUI-selected config.
        _ => build_executor_claude(),
    };

    let count = task_ids.len();
    if count > max_parallel {
        output::print_success(&format!(
            "queuing {count} task(s) (max_parallel={max_parallel})"
        ));
    } else {
        output::print_success(&format!("spawning {count} task(s)..."));
    }

    let detach = args.detach;
    let no_tmux = args.no_tmux;

    match executor {
        ExecutorKind::Claude(exec) => {
            let mut scheduler =
                tc_spawn::scheduler::Scheduler::new(exec, worktree_mgr, max_parallel);
            if no_tmux {
                scheduler.use_tmux = false;
            }
            drive_workers(scheduler, task_ids, &store, &config, detach).await
        }
        ExecutorKind::Opencode(exec) => {
            let mut scheduler =
                tc_spawn::scheduler::Scheduler::new(exec, worktree_mgr, max_parallel);
            if no_tmux {
                scheduler.use_tmux = false;
            }
            drive_workers(scheduler, task_ids, &store, &config, detach).await
        }
    }
}

async fn drive_workers<E: tc_executor::traits::Executor>(
    mut scheduler: tc_spawn::scheduler::Scheduler<E>,
    task_ids: Vec<TaskId>,
    store: &tc_storage::Store,
    config: &tc_core::config::TcConfig,
    detach: bool,
) -> Result<(), CliError> {
    // Pre-flight: validate the full queue for file conflicts across all tasks,
    // not just per-batch. This catches conflicts that would otherwise be masked
    // by batching.
    let all_tasks = store.load_tasks().map_err(CliError::from)?;
    let queue_tasks: Vec<&tc_core::task::Task> = task_ids
        .iter()
        .filter_map(|id| all_tasks.iter().find(|t| t.id == *id))
        .collect();
    tc_spawn::scheduler::Scheduler::<E>::validate_queue(&queue_tasks).map_err(CliError::from)?;

    let max_parallel = scheduler.max_parallel;
    let mut queue: VecDeque<TaskId> = task_ids.into();

    // Detach: spawn only the first batch and exit. Any tasks beyond the first
    // batch are dropped -- detach is fire-and-forget and can't re-drive the
    // queue after the CLI exits.
    if detach {
        let free_slots = max_parallel.saturating_sub(scheduler.active_count());
        let batch_size = free_slots.min(queue.len());
        if batch_size > 0 {
            let batch: Vec<TaskId> = queue.drain(..batch_size).collect();
            let spawned = scheduler
                .spawn_tasks(batch, store, config)
                .await
                .map_err(CliError::from)?;
            if spawned > 0 {
                output::print_success(&format!("{spawned} worker(s) spawned"));
            }
        }
        if !queue.is_empty() {
            output::print_success(&format!(
                "note: {} queued task(s) dropped (detach mode -- re-run without --detach to drain the queue)",
                queue.len()
            ));
        }
        return Ok(());
    }

    while !queue.is_empty() || scheduler.active_count() > 0 {
        let free_slots = max_parallel.saturating_sub(scheduler.active_count());
        let batch_size = free_slots.min(queue.len());
        if batch_size > 0 {
            let batch: Vec<TaskId> = queue.drain(..batch_size).collect();
            let spawned = scheduler
                .spawn_tasks(batch, store, config)
                .await
                .map_err(CliError::from)?;
            if spawned > 0 {
                output::print_success(&format!("{spawned} worker(s) spawned"));
            }
        }

        let done = scheduler
            .poll_workers(store, config)
            .await
            .map_err(CliError::from)?;
        for id in &done {
            output::print_success(&format!("{} finished", id.0));
        }

        if queue.is_empty() && scheduler.active_count() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    Ok(())
}

pub fn run_workers(args: WorkersArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let config = store.load_config()?;
    let workers_dir = store.workers_dir();

    if args.cleanup {
        let worktree_mgr = WorktreeManager::new(store.root().clone(), config.spawn.clone());
        let cleaned = recovery::cleanup_orphans(&store, &worktree_mgr, true)?;
        if cleaned.is_empty() {
            output::print_success("no orphaned workers found");
        } else {
            output::print_success(&format!(
                "cleaned up {} orphaned worker(s): {}",
                cleaned.len(),
                cleaned.join(", ")
            ));
        }
        return Ok(());
    }

    // Refresh states (mark dead PIDs as failed)
    let states = recovery::refresh_worker_states(&workers_dir)?;

    if states.is_empty() {
        println!("No active workers.");
        return Ok(());
    }

    print_workers_table(&states);

    Ok(())
}

pub fn run_logs(args: LogsArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let task_id = TaskId(args.id.clone());
    let log_path = store.log_path(&task_id);

    if !log_path.exists() {
        return Err(CliError::user(format!("no log file found for {}", args.id)));
    }

    if args.follow {
        follow_log(&log_path)?;
    } else {
        let content = std::fs::read_to_string(&log_path)
            .map_err(|e| CliError::user(format!("failed to read log: {e}")))?;
        print!("{content}");
    }

    Ok(())
}

pub fn run_kill(args: KillArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let workers_dir = store.workers_dir();

    if args.all {
        return kill_all_workers(&workers_dir);
    }

    let id = args
        .id
        .ok_or_else(|| CliError::user("task ID required (or use --all)"))?;

    kill_worker(&workers_dir, &id)
}

pub fn run_attach(args: AttachArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let workers_dir = store.workers_dir();
    let state_path = workers_dir.join(format!("{}.json", args.id));

    if !state_path.exists() {
        return Err(CliError::user(format!("no worker found for {}", args.id)));
    }

    let state = WorkerState::load(&state_path)?;

    let session = state.tmux_session.ok_or_else(|| {
        CliError::user(format!(
            "worker {} was not spawned in a tmux session",
            args.id
        ))
    })?;

    if !tc_spawn::tmux::has_session(&session) {
        return Err(CliError::user(format!(
            "tmux session '{}' no longer exists (worker may have finished)",
            session
        )));
    }

    // This replaces the current process via exec
    tc_spawn::tmux::attach_session(&session)?;
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────

enum ExecutorKind {
    Claude(tc_executor::claude::ClaudeExecutor),
    Opencode(tc_executor::opencode::OpencodeExecutor),
}

fn build_executor_claude() -> ExecutorKind {
    ExecutorKind::Claude(tc_executor::claude::ClaudeExecutor)
}

fn build_executor_opencode() -> ExecutorKind {
    ExecutorKind::Opencode(tc_executor::opencode::OpencodeExecutor)
}

fn print_workers_table(states: &[WorkerState]) {
    let use_color = output::colors_enabled();

    // Header
    let header = format!(
        "{:<8}  {:<10}  {:<8}  {:<20}  {:<14}  WORKTREE",
        "TASK", "STATUS", "PID", "STARTED", "TMUX"
    );
    println!("{header}");
    println!("{}", "-".repeat(92));

    for state in states {
        let status_str = if use_color {
            output::colored_status_str(&state.status.to_string())
        } else {
            state.status.to_string()
        };

        let tmux_str = state.tmux_session.as_deref().unwrap_or("-");

        println!(
            "{:<8}  {:<10}  {:<8}  {:<20}  {:<14}  {}",
            state.task_id,
            status_str,
            state.pid,
            state.started_at.format("%Y-%m-%d %H:%M:%S"),
            tmux_str,
            state.worktree_path,
        );
    }
}

fn follow_log(log_path: &std::path::Path) -> Result<(), CliError> {
    let interrupted = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&interrupted))
        .map_err(|e| CliError::user(format!("failed to register signal handler: {e}")))?;

    let mut stdout = std::io::stdout().lock();
    log_tail::follow_to_writer(log_path, &mut stdout, &interrupted)
        .map_err(|e| CliError::user(format!("failed to tail log: {e}")))
}

fn send_sigterm_and_mark_killed(
    state: &mut WorkerState,
    state_path: &std::path::Path,
) -> Result<(), CliError> {
    if let Some(ref session) = state.tmux_session {
        let _ = tc_spawn::tmux::kill_session(session);
    } else {
        unsafe {
            libc::kill(state.pid as libc::pid_t, libc::SIGTERM);
        }
    }
    state.status = tc_spawn::process::WorkerStatus::Killed;
    state.save(state_path)?;
    Ok(())
}

fn kill_worker(workers_dir: &std::path::Path, task_id: &str) -> Result<(), CliError> {
    let state_path = workers_dir.join(format!("{task_id}.json"));
    if !state_path.exists() {
        return Err(CliError::user(format!("no worker found for {task_id}")));
    }

    let mut state = WorkerState::load(&state_path)?;

    if state.status != tc_spawn::process::WorkerStatus::Running {
        return Err(CliError::user(format!(
            "worker {task_id} is not running (status: {})",
            state.status
        )));
    }

    let pid = state.pid;
    send_sigterm_and_mark_killed(&mut state, &state_path)?;
    output::print_success(&format!("killed worker {task_id} (pid {pid})"));
    Ok(())
}

fn kill_all_workers(workers_dir: &std::path::Path) -> Result<(), CliError> {
    let states = list_worker_states(workers_dir)?;
    let killed = states
        .into_iter()
        .filter(|s| s.status == tc_spawn::process::WorkerStatus::Running)
        .filter(|s| {
            let state_path = workers_dir.join(format!("{}.json", s.task_id));
            let mut updated = s.clone();
            send_sigterm_and_mark_killed(&mut updated, &state_path).is_ok()
        })
        .count();

    output::print_success(&format!("killed {killed} worker(s)"));
    Ok(())
}
