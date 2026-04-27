use std::path::Path;

use tc_core::status::StatusId;
use tc_core::task::TaskId;
use tc_storage::Store;

use crate::error::SpawnError;
use crate::process::{WorkerState, WorkerStatus};
use crate::scheduler::list_worker_states;
use crate::tmux;
use crate::worktree::WorktreeManager;

/// Information about an orphaned worker.
#[derive(Debug)]
pub struct OrphanedWorker {
    pub task_id: String,
    pub pid: u32,
    pub worktree_path: String,
}

/// Scan worker state files and check PID liveness.
///
/// Returns workers whose PID is no longer running.
pub fn scan_orphaned_workers(workers_dir: &Path) -> Result<Vec<OrphanedWorker>, SpawnError> {
    let states = list_worker_states(workers_dir)?;
    let mut orphaned = Vec::new();

    'scan: for state in states {
        if state.status != WorkerStatus::Running {
            continue 'scan;
        }

        let alive = if let Some(ref session) = state.tmux_session {
            tmux::has_session(session)
        } else {
            is_pid_alive(state.pid)
        };

        if !alive {
            orphaned.push(OrphanedWorker {
                task_id: state.task_id,
                pid: state.pid,
                worktree_path: state.worktree_path,
            });
        }
    }

    Ok(orphaned)
}

/// Clean up orphaned workers: reset task status, optionally remove worktrees, remove state files.
pub fn cleanup_orphans(
    store: &Store,
    worktree_mgr: &WorktreeManager,
    remove_worktrees: bool,
) -> Result<Vec<String>, SpawnError> {
    let orphaned = scan_orphaned_workers(&store.workers_dir())?;
    let mut cleaned = Vec::new();

    if orphaned.is_empty() {
        return Ok(cleaned);
    }

    let mut tasks = store.load_tasks()?;

    for orphan in &orphaned {
        if let Some(task) = tasks.iter_mut().find(|t| t.id.0 == orphan.task_id) {
            task.status = StatusId("todo".into());
            if !task.notes.is_empty() {
                task.notes.push('\n');
            }
            task.notes
                .push_str("RESET: worker process died unexpectedly");
        }

        if remove_worktrees {
            let task_id = TaskId(orphan.task_id.clone());
            let _ = worktree_mgr.remove(&task_id);
        }

        let state_path = store.worker_state_path(&TaskId(orphan.task_id.clone()));
        let _ = std::fs::remove_file(&state_path);

        cleaned.push(orphan.task_id.clone());
    }

    store.save_tasks(&tasks)?;

    Ok(cleaned)
}

/// Update worker state files with current liveness info.
///
/// Marks workers with dead PIDs as "failed" in their state file.
pub fn refresh_worker_states(workers_dir: &Path) -> Result<Vec<WorkerState>, SpawnError> {
    let mut states = list_worker_states(workers_dir)?;

    'refresh: for state in &mut states {
        if state.status != WorkerStatus::Running {
            continue 'refresh;
        }

        let alive = if let Some(ref session) = state.tmux_session {
            tmux::has_session(session)
        } else {
            is_pid_alive(state.pid)
        };

        if !alive {
            state.status = WorkerStatus::Failed;
            let state_path = workers_dir.join(format!("{}.json", state.task_id));
            let _ = state.save(&state_path);
        }
    }

    Ok(states)
}

/// Reconciled worker transition -- one of these per task whose status changed
/// from Running to a terminal state during reconcile.
#[derive(Debug, Clone)]
pub struct WorkerTransition {
    pub task_id: String,
    pub status: WorkerStatus,
    pub exit_code: Option<i32>,
}

/// Reconcile active worker state files against process/session liveness,
/// updating both the `WorkerState` JSONs and the parent `tasks.yaml`.
///
/// For each worker with status=Running:
///   - Tmux sessions: if the session is gone, read the exit-code file.
///     Exit=0 -> Completed + task moves to `review`. Non-zero -> Failed +
///     task moves to `blocked` with a note. Missing exit code treated as -1.
///   - Direct processes: if the PID is dead, marks Failed + task blocked
///     (no exit code available without the `tokio::process::Child` handle).
///
/// Returns both the transitions that occurred and the final state of all
/// workers after reconciliation, so callers avoid a second filesystem scan.
///
/// Unlike [`Scheduler::poll_workers`], this function is stateless and does
/// not perform auto-commit or verification. It is intended for the TUI, which
/// does not keep `WorkerHandle`s across spawns.
pub fn reconcile_workers(
    store: &Store,
) -> Result<(Vec<WorkerTransition>, Vec<WorkerState>), SpawnError> {
    let workers_dir = store.workers_dir();
    let raw_states = list_worker_states(&workers_dir)?;
    let mut transitions: Vec<WorkerTransition> = Vec::new();
    let mut final_states: Vec<WorkerState> = Vec::new();

    'scan: for mut state in raw_states {
        if state.status != WorkerStatus::Running {
            final_states.push(state);
            continue 'scan;
        }

        let alive = if let Some(ref session) = state.tmux_session {
            tmux::has_session(session)
        } else {
            is_pid_alive(state.pid)
        };
        if alive {
            final_states.push(state);
            continue 'scan;
        }

        let exit_code = state.tmux_session.as_ref().and_then(|_| {
            let task_id = TaskId(state.task_id.clone());
            tmux::read_exit_code(&store.worker_exit_code_path(&task_id))
        });
        let new_status = match exit_code {
            Some(0) => WorkerStatus::Completed,
            _ => WorkerStatus::Failed,
        };
        state.status = new_status.clone();

        let state_path = store.worker_state_path(&TaskId(state.task_id.clone()));
        let _ = state.save(&state_path);

        transitions.push(WorkerTransition {
            task_id: state.task_id.clone(),
            status: new_status,
            exit_code,
        });
        final_states.push(state);
    }

    if transitions.is_empty() {
        return Ok((transitions, final_states));
    }

    let mut tasks = store.load_tasks()?;
    'apply: for t in &transitions {
        let Some(task) = tasks.iter_mut().find(|x| x.id.0 == t.task_id) else {
            continue 'apply;
        };
        match t.status {
            WorkerStatus::Completed => {
                task.status = StatusId::review();
            }
            WorkerStatus::Failed => {
                task.status = StatusId::blocked();
                let note = match t.exit_code {
                    Some(code) => format!("BLOCKED: worker exited with code {code}"),
                    None => "BLOCKED: worker process ended unexpectedly".to_string(),
                };
                if !task.notes.is_empty() {
                    task.notes.push('\n');
                }
                task.notes.push_str(&note);
            }
            WorkerStatus::Running | WorkerStatus::Killed => {}
        }
    }
    store.save_tasks(&tasks)?;

    Ok((transitions, final_states))
}

/// Check if a PID is still alive using kill(pid, 0).
fn is_pid_alive(pid: u32) -> bool {
    // SAFETY: signal 0 doesn't actually send a signal, just checks if
    // the process exists and we have permission to signal it.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::WorkerState;
    use tempfile::TempDir;

    #[test]
    fn scan_empty_dir() {
        let dir = TempDir::new().unwrap();
        let orphaned = scan_orphaned_workers(dir.path()).unwrap();
        assert!(orphaned.is_empty());
    }

    #[test]
    fn scan_nonexistent_dir() {
        let orphaned = scan_orphaned_workers(Path::new("/nonexistent/workers")).unwrap();
        assert!(orphaned.is_empty());
    }

    #[test]
    fn scan_detects_dead_pid() {
        let dir = TempDir::new().unwrap();
        let state = WorkerState {
            task_id: "T-001".into(),
            pid: 999_999_999, // Very unlikely to be alive
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt/T-001".into(),
            status: WorkerStatus::Running,
            log_path: "/tmp/logs/T-001.log".into(),
            tmux_session: None,
        };
        state.save(&dir.path().join("T-001.json")).unwrap();

        let orphaned = scan_orphaned_workers(dir.path()).unwrap();
        assert_eq!(orphaned.len(), 1);
        assert_eq!(orphaned[0].task_id, "T-001");
    }

    #[test]
    fn scan_skips_completed_workers() {
        let dir = TempDir::new().unwrap();
        let state = WorkerState {
            task_id: "T-002".into(),
            pid: 999_999_999,
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt/T-002".into(),
            status: WorkerStatus::Completed,
            log_path: "/tmp/logs/T-002.log".into(),
            tmux_session: None,
        };
        state.save(&dir.path().join("T-002.json")).unwrap();

        let orphaned = scan_orphaned_workers(dir.path()).unwrap();
        assert!(orphaned.is_empty());
    }

    #[test]
    fn current_pid_is_alive() {
        assert!(is_pid_alive(std::process::id()));
    }

    #[test]
    fn bogus_pid_is_not_alive() {
        assert!(!is_pid_alive(999_999_999));
    }

    #[test]
    fn refresh_marks_dead_as_failed() {
        let dir = TempDir::new().unwrap();
        let state = WorkerState {
            task_id: "T-003".into(),
            pid: 999_999_999,
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt/T-003".into(),
            status: WorkerStatus::Running,
            log_path: "/tmp/logs/T-003.log".into(),
            tmux_session: None,
        };
        state.save(&dir.path().join("T-003.json")).unwrap();

        let states = refresh_worker_states(dir.path()).unwrap();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].status, WorkerStatus::Failed);

        // Verify persisted
        let reloaded = WorkerState::load(&dir.path().join("T-003.json")).unwrap();
        assert_eq!(reloaded.status, WorkerStatus::Failed);
    }

    fn init_store_with_task(dir: &Path, task_id: &str) -> Store {
        let store = tc_storage::init::init_project(dir).unwrap();
        let task = tc_core::task::Task {
            id: TaskId(task_id.into()),
            title: format!("Task {task_id}"),
            epic: "test".into(),
            status: StatusId::in_progress(),
            priority: tc_core::task::Priority::default(),
            tags: vec![],
            due: None,
            scheduled: None,
            estimate: None,
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: chrono::Utc::now(),
        };
        store.save_tasks(&[task]).unwrap();
        store
    }

    #[test]
    fn reconcile_skips_running_workers() {
        let dir = TempDir::new().unwrap();
        let store = init_store_with_task(dir.path(), "T-040");

        let state = WorkerState {
            task_id: "T-040".into(),
            pid: std::process::id(),
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt/T-040".into(),
            status: WorkerStatus::Running,
            log_path: "/tmp/logs/T-040.log".into(),
            tmux_session: None,
        };
        state.save(&store.workers_dir().join("T-040.json")).unwrap();

        let (transitions, _states) = reconcile_workers(&store).unwrap();
        assert!(transitions.is_empty());
        let tasks = store.load_tasks().unwrap();
        assert_eq!(tasks[0].status, StatusId::in_progress());
    }

    #[test]
    fn reconcile_marks_dead_process_worker_failed() {
        let dir = TempDir::new().unwrap();
        let store = init_store_with_task(dir.path(), "T-041");

        let state = WorkerState {
            task_id: "T-041".into(),
            pid: 999_999_999,
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt/T-041".into(),
            status: WorkerStatus::Running,
            log_path: "/tmp/logs/T-041.log".into(),
            tmux_session: None,
        };
        state.save(&store.workers_dir().join("T-041.json")).unwrap();

        let (transitions, _states) = reconcile_workers(&store).unwrap();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].task_id, "T-041");
        assert_eq!(transitions[0].status, WorkerStatus::Failed);

        let reloaded = WorkerState::load(&store.workers_dir().join("T-041.json")).unwrap();
        assert_eq!(reloaded.status, WorkerStatus::Failed);

        let tasks = store.load_tasks().unwrap();
        assert_eq!(tasks[0].status, StatusId::blocked());
        assert!(tasks[0].notes.contains("BLOCKED"));
    }

    #[test]
    fn reconcile_marks_tmux_exit_zero_completed() {
        let dir = TempDir::new().unwrap();
        let store = init_store_with_task(dir.path(), "T-042");

        let state = WorkerState {
            task_id: "T-042".into(),
            pid: 999_999_999,
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt/T-042".into(),
            status: WorkerStatus::Running,
            log_path: "/tmp/logs/T-042.log".into(),
            tmux_session: Some("tc-T-042-nonexistent-xyz".into()),
        };
        state.save(&store.workers_dir().join("T-042.json")).unwrap();

        // Write exit-code file as if the tmux wrapper had exited cleanly.
        std::fs::write(store.worker_exit_code_path(&TaskId("T-042".into())), "0\n").unwrap();

        let (transitions, _states) = reconcile_workers(&store).unwrap();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].status, WorkerStatus::Completed);
        assert_eq!(transitions[0].exit_code, Some(0));

        let tasks = store.load_tasks().unwrap();
        assert_eq!(tasks[0].status, StatusId::review());
    }

    #[test]
    fn reconcile_marks_tmux_exit_nonzero_failed() {
        let dir = TempDir::new().unwrap();
        let store = init_store_with_task(dir.path(), "T-043");

        let state = WorkerState {
            task_id: "T-043".into(),
            pid: 999_999_999,
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt/T-043".into(),
            status: WorkerStatus::Running,
            log_path: "/tmp/logs/T-043.log".into(),
            tmux_session: Some("tc-T-043-nonexistent-xyz".into()),
        };
        state.save(&store.workers_dir().join("T-043.json")).unwrap();

        std::fs::write(store.worker_exit_code_path(&TaskId("T-043".into())), "2\n").unwrap();

        let (transitions, _states) = reconcile_workers(&store).unwrap();
        assert_eq!(transitions.len(), 1);
        assert_eq!(transitions[0].status, WorkerStatus::Failed);
        assert_eq!(transitions[0].exit_code, Some(2));

        let tasks = store.load_tasks().unwrap();
        assert_eq!(tasks[0].status, StatusId::blocked());
        assert!(tasks[0].notes.contains("exited with code 2"));
    }
}
