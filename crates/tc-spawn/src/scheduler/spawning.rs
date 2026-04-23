use std::path::Path;

use tc_core::config::TcConfig;
use tc_core::context::{ContextRenderer, build_resolved_deps};
use tc_core::dag::TaskDag;
use tc_core::status::{StatusId, StatusMachine};
use tc_core::task::{Task, TaskId};
use tc_executor::sandbox::sandbox_from_core;
use tc_executor::traits::{ExecutionMode, ExecutionRequest, Executor};
use tc_storage::Store;

use crate::error::SpawnError;
use crate::process::{WorkerHandle, WorkerState};
use crate::scheduler::core::Scheduler;
use crate::tmux;

impl<E: Executor> Scheduler<E> {
    /// Spawn tasks as parallel workers in worktrees.
    ///
    /// Respects `max_parallel` -- spawns at most `max_parallel - active_count`
    /// workers from the given batch. Any tasks beyond the available slots are
    /// silently skipped (the caller is expected to re-queue them).
    /// Returns the number of tasks actually spawned (may be 0 if no slots free).
    pub async fn spawn_tasks(
        &mut self,
        task_ids: Vec<TaskId>,
        store: &Store,
        config: &TcConfig,
    ) -> Result<usize, SpawnError> {
        let tasks = store.load_tasks()?;
        let sm = StatusMachine::new(config.statuses.clone());

        let spawn_tasks: Vec<&Task> = task_ids
            .iter()
            .filter_map(|id| tasks.iter().find(|t| t.id == *id))
            .collect();

        if spawn_tasks.is_empty() {
            return Err(SpawnError::NoReadyTasks);
        }

        let available_slots = self.max_parallel.saturating_sub(self.workers.len());
        let to_spawn = spawn_tasks.len().min(available_slots);

        if to_spawn == 0 {
            return Ok(0);
        }

        let dag = TaskDag::from_tasks(&tasks)?;
        let renderer = ContextRenderer::new(&config.context_template)?;
        let sandbox = sandbox_from_core(&config.executor.sandbox);

        let mut updated_tasks = tasks.clone();
        let mut spawned = 0;

        for task in &spawn_tasks[..to_spawn] {
            let plan = SpawnPlan::prepare(self, store, task, &tasks, &dag, &sm, &renderer)?;
            let handle = self.launch_worker(store, task, &plan, &sandbox)?;

            if let Some(t) = updated_tasks.iter_mut().find(|t| t.id == task.id) {
                t.status = StatusId::in_progress();
            }

            self.workers.push(handle);
            spawned += 1;
        }

        if spawned > 0 {
            store.save_tasks(&updated_tasks)?;
        }

        Ok(spawned)
    }

    fn launch_worker(
        &mut self,
        store: &Store,
        task: &Task,
        plan: &SpawnPlan,
        sandbox: &tc_executor::traits::SandboxConfig,
    ) -> Result<WorkerHandle, SpawnError> {
        let request = ExecutionRequest {
            context: plan.context.clone(),
            mode: ExecutionMode::Yolo,
            working_dir: plan.wt_path.clone(),
            sandbox: sandbox.clone(),
            mcp_servers: vec![],
        };

        if self.use_tmux {
            self.launch_tmux(store, task, plan, &request)
        } else {
            self.launch_direct(store, task, plan, &request)
        }
    }

    fn launch_tmux(
        &self,
        store: &Store,
        task: &Task,
        plan: &SpawnPlan,
        request: &ExecutionRequest,
    ) -> Result<WorkerHandle, SpawnError> {
        let cmd = self.executor.build_command(request)?;
        let std_cmd = cmd.as_std();
        let program = std_cmd.get_program().to_string_lossy().to_string();
        let args: Vec<String> = std_cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();

        let session = tmux::session_name(&task.id.0);
        let shell_cmd = tmux::build_shell_command(&program, &args);
        let exit_code_path = store.worker_exit_code_path(&task.id);

        std::fs::File::create(&plan.log_path).map_err(|e| SpawnError::io("create log file", e))?;

        tmux::create_session(
            &session,
            &shell_cmd,
            &plan.wt_path,
            &plan.log_path,
            &exit_code_path,
        )?;

        let pid = tmux::session_pid(&session).unwrap_or(0);
        let state = WorkerState::new_tmux(
            &task.id,
            pid,
            &plan.wt_path,
            &plan.log_path,
            session.clone(),
        );
        let state_path = store.worker_state_path(&task.id);
        state.save(&state_path)?;

        Ok(WorkerHandle::new_tmux(
            task.id.clone(),
            plan.log_path.clone(),
            session,
            exit_code_path,
        ))
    }

    fn launch_direct(
        &self,
        store: &Store,
        task: &Task,
        plan: &SpawnPlan,
        request: &ExecutionRequest,
    ) -> Result<WorkerHandle, SpawnError> {
        let mut cmd = self.executor.build_command(request)?;

        let log_file = std::fs::File::create(&plan.log_path)
            .map_err(|e| SpawnError::io("create log file", e))?;
        let stderr_file = log_file
            .try_clone()
            .map_err(|e| SpawnError::io("clone log file", e))?;

        cmd.stdout(std::process::Stdio::from(log_file));
        cmd.stderr(std::process::Stdio::from(stderr_file));

        let child = cmd
            .spawn()
            .map_err(|e| SpawnError::worker_spawn(&task.id.0, e))?;

        let pid = child.id().unwrap_or(0);
        let state = WorkerState::new(&task.id, pid, &plan.wt_path, &plan.log_path);
        let state_path = store.worker_state_path(&task.id);
        state.save(&state_path)?;

        Ok(WorkerHandle::new(
            task.id.clone(),
            plan.log_path.clone(),
            child,
        ))
    }
}

/// Materialized inputs for one worker launch -- worktree, log sink and
/// rendered prompt context. Isolated from `spawn_tasks` so each task in the
/// batch walks the same path and `launch_worker` stays readable.
struct SpawnPlan {
    wt_path: std::path::PathBuf,
    log_path: std::path::PathBuf,
    context: String,
}

impl SpawnPlan {
    fn prepare<E: Executor>(
        sched: &mut Scheduler<E>,
        store: &Store,
        task: &Task,
        tasks: &[Task],
        dag: &TaskDag,
        sm: &StatusMachine,
        renderer: &ContextRenderer,
    ) -> Result<Self, SpawnError> {
        let wt_path = sched.worktree_mgr.create(&task.id)?;
        let log_path = store.log_path(&task.id);

        if let Some(log_parent) = log_path.parent() {
            std::fs::create_dir_all(log_parent).map_err(|e| SpawnError::io("create log dir", e))?;
        }

        let resolved_deps = build_resolved_deps(tasks, dag, &task.id, sm);
        let context = renderer.render(task, &resolved_deps, None)?;

        Ok(Self {
            wt_path,
            log_path,
            context,
        })
    }
}

/// Auto-commit any pending changes in a worktree.
///
/// Stages all changes with `git add -A`, then commits with a message
/// referencing the task. No-op if there are no staged changes.
pub(crate) fn auto_commit_worktree(
    working_dir: &Path,
    task_id: &TaskId,
    title: &str,
) -> Result<(), SpawnError> {
    let add_status = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(working_dir)
        .status()
        .map_err(|e| SpawnError::git("git add", e.to_string()))?;
    if !add_status.success() {
        return Err(SpawnError::git(
            "git add",
            format!("exit code {:?}", add_status.code()),
        ));
    }

    let diff_status = std::process::Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(working_dir)
        .status()
        .map_err(|e| SpawnError::git("git diff --cached", e.to_string()))?;

    // Exit code 0 = no staged changes, exit code 1 = staged changes present
    if diff_status.success() {
        return Ok(());
    }

    let message = format!("tc: {} {}", task_id.0, title);
    let commit_output = std::process::Command::new("git")
        .args(["commit", "-m", &message])
        .current_dir(working_dir)
        .output()
        .map_err(|e| SpawnError::git("git commit", e.to_string()))?;
    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        return Err(SpawnError::git("git commit", stderr.trim().to_string()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_git_repo(dir: &Path) {
        let cmds: &[&[&str]] = &[
            &["init", "-q"],
            &["config", "user.email", "test@test.com"],
            &["config", "user.name", "Test"],
            &["checkout", "-q", "-b", "main"],
        ];
        for args in cmds {
            std::process::Command::new("git")
                .args(*args)
                .current_dir(dir)
                .output()
                .unwrap();
        }
        std::fs::write(dir.join("README.md"), "# init\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn auto_commit_creates_commit_when_changes_present() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());

        std::fs::write(dir.path().join("new.txt"), "hello").unwrap();

        let task_id = TaskId("T-001".into());
        auto_commit_worktree(dir.path(), &task_id, "Test task").unwrap();

        let log = std::process::Command::new("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        assert!(log_str.contains("tc: T-001 Test task"), "log: {log_str}");

        let status = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(status.stdout.is_empty());
    }

    #[test]
    fn auto_commit_is_noop_when_clean() {
        let dir = tempfile::tempdir().unwrap();
        init_git_repo(dir.path());

        let count_before = std::process::Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let count_before_str = String::from_utf8_lossy(&count_before.stdout)
            .trim()
            .to_string();

        let task_id = TaskId("T-002".into());
        auto_commit_worktree(dir.path(), &task_id, "noop").unwrap();

        let count_after = std::process::Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let count_after_str = String::from_utf8_lossy(&count_after.stdout)
            .trim()
            .to_string();

        assert_eq!(count_before_str, count_after_str);
    }
}
