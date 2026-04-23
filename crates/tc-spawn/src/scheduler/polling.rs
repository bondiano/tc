use tc_core::config::TcConfig;
use tc_core::status::StatusId;
use tc_core::task::TaskId;
use tc_executor::traits::Executor;
use tc_executor::verify;
use tc_storage::Store;

use crate::error::SpawnError;
use crate::process::{WorkerState, WorkerStatus};
use crate::scheduler::core::Scheduler;
use crate::scheduler::spawning::auto_commit_worktree;

impl<E: Executor> Scheduler<E> {
    /// Poll all active workers, handling completions.
    ///
    /// Returns task IDs of workers that completed during this poll.
    pub async fn poll_workers(
        &mut self,
        store: &Store,
        config: &TcConfig,
    ) -> Result<Vec<TaskId>, SpawnError> {
        let mut completed = Vec::new();

        let mut i = 0;
        while i < self.workers.len() {
            if self.workers[i].is_running() {
                i += 1;
                continue;
            }

            let mut handle = self.workers.remove(i);
            let task_id = handle.task_id().clone();
            let result = handle.wait().await?;

            update_worker_state_file(store, &task_id, result.exit_code);
            self.finalize_task(store, config, &task_id, result.exit_code)
                .await?;

            completed.push(task_id);
        }

        Ok(completed)
    }

    async fn finalize_task(
        &self,
        store: &Store,
        config: &TcConfig,
        task_id: &TaskId,
        exit_code: i32,
    ) -> Result<(), SpawnError> {
        let mut tasks = store.load_tasks()?;
        let Some(task) = tasks.iter_mut().find(|t| t.id == *task_id) else {
            store.save_tasks(&tasks)?;
            return Ok(());
        };

        if exit_code != 0 {
            task.status = StatusId::blocked();
            let note = format!("BLOCKED: agent exited with code {exit_code}");
            if !task.notes.is_empty() {
                task.notes.push('\n');
            }
            task.notes.push_str(&note);
            store.save_tasks(&tasks)?;
            return Ok(());
        }

        let wt_info = self.worktree_mgr.find(task_id)?;
        let working_dir = wt_info
            .map(|w| w.path)
            .unwrap_or_else(|| store.root().clone());

        if config.spawn.auto_commit {
            auto_commit_worktree(&working_dir, task_id, &task.title)?;
        }

        let new_status = resolve_post_run_status(config, &working_dir).await?;
        task.status = new_status;
        store.save_tasks(&tasks)?;
        Ok(())
    }
}

fn update_worker_state_file(store: &Store, task_id: &TaskId, exit_code: i32) {
    let state_path = store.worker_state_path(task_id);
    if let Ok(mut state) = WorkerState::load(&state_path) {
        state.status = if exit_code == 0 {
            WorkerStatus::Completed
        } else {
            WorkerStatus::Failed
        };
        let _ = state.save(&state_path);
    }
}

async fn resolve_post_run_status(
    config: &TcConfig,
    working_dir: &std::path::Path,
) -> Result<StatusId, SpawnError> {
    if config.verification.commands.is_empty() {
        return Ok(StatusId::review());
    }

    let verify_result =
        verify::run_verification(&config.verification.commands, working_dir).await?;

    let id = if verify_result.passed {
        config.verification.on_pass.clone()
    } else {
        config.verification.on_fail.clone()
    };
    Ok(StatusId(id))
}
