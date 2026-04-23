use tc_core::config::TcConfig;
use tc_core::status::StatusId;
use tc_core::task::Task;
use tc_executor::any::executor_by_name;
use tc_spawn::merge::{MergeResult, merge_worktree};
use tc_spawn::process::{WorkerState, WorkerStatus};
use tc_spawn::worktree::WorktreeManager;

use crate::create_task::CreateTaskForm;
use crate::error::{TuiError, TuiResult};

use super::ALL_EPIC;
use super::types::{App, AppScreen, TuiAction};

impl App {
    pub(super) fn action_done(&mut self) -> TuiResult<()> {
        let Some(t) = self.selected_task() else {
            return Ok(());
        };
        let mut new_tasks = self.tasks.clone();
        if let Some(target) = new_tasks.iter_mut().find(|x| x.id == t.id) {
            target.status = StatusId("done".into());
        }
        self.store.save_tasks(&new_tasks)?;
        self.reload_tasks()?;
        self.toast(&format!("marked {} done", t.id.0));
        Ok(())
    }

    pub(super) fn action_delete(&mut self) {
        let Some(t) = self.selected_task() else {
            return;
        };
        self.pending_delete = Some(t);
        self.confirm_delete_yes = false;
    }

    pub(super) fn execute_delete(&mut self) -> TuiResult<()> {
        let Some(t) = self.pending_delete.take() else {
            return Ok(());
        };
        let new_tasks: Vec<Task> = self
            .tasks
            .iter()
            .filter(|task| task.id != t.id)
            .cloned()
            .collect();
        self.store.save_tasks(&new_tasks)?;
        self.reload_tasks()?;
        self.toast(&format!("deleted {}", t.id.0));
        Ok(())
    }

    /// Suspend TUI and launch interactive claude for the selected task.
    pub(super) fn action_impl(&mut self) -> TuiResult<()> {
        let Some(task) = self.selected_task() else {
            self.toast("start: no task selected");
            return Ok(());
        };
        if self.status_machine.is_terminal(&task.status) {
            self.toast(&format!("start: {} is already done", task.id));
            return Ok(());
        }
        self.pending_action = TuiAction::SuspendForImpl(task.id);
        Ok(())
    }

    /// Spawn the selected task in a worktree (headless, stay in TUI).
    pub(super) fn action_spawn(&mut self) -> TuiResult<()> {
        let Some(task) = self.selected_task() else {
            self.toast("run: no task selected");
            return Ok(());
        };
        if self.status_machine.is_terminal(&task.status) {
            self.toast(&format!("run: {} is already done", task.id));
            return Ok(());
        }
        if self.worker_for(&task.id).is_some() {
            self.toast(&format!("run: {} is already running", task.id));
            return Ok(());
        }

        let worktree_mgr =
            WorktreeManager::new(self.store.root().clone(), self.config.spawn.clone());

        let chosen = resolve_executor_name(&self.config.executor.default, &self.config);
        let executor = executor_by_name(&chosen, &self.config)
            .map_err(|e| TuiError::Render(format!("unknown executor: {e}")))?;

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| TuiError::Render(format!("failed to create runtime: {e}")))?;

        let task_id = task.id.clone();
        let mut scheduler =
            tc_spawn::scheduler::Scheduler::new(executor, worktree_mgr, self.max_workers);
        let result =
            rt.block_on(scheduler.spawn_tasks(vec![task_id.clone()], &self.store, &self.config));

        match result {
            Ok(count) => {
                self.reload_tasks()?;
                self.refresh_workers();
                if count == 0 {
                    self.toast(&format!("could not start {}", task_id));
                } else {
                    self.toast(&format!("started {} in background", task_id));
                }
            }
            Err(e) => {
                self.toast(&format!("failed to start: {e}"));
            }
        }
        Ok(())
    }

    /// Kill the worker for the selected task.
    pub(super) fn action_kill(&mut self) -> TuiResult<()> {
        let Some(task) = self.selected_task() else {
            self.toast("stop: no task selected");
            return Ok(());
        };
        let Some(worker) = self.worker_for(&task.id) else {
            self.toast(&format!("stop: {} is not running", task.id));
            return Ok(());
        };
        if worker.status != WorkerStatus::Running {
            self.toast(&format!("stop: {} is not running", task.id));
            return Ok(());
        }

        let pid = worker.pid;
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }

        let state_path = self.store.worker_state_path(&task.id);
        if let Ok(mut state) = WorkerState::load(&state_path) {
            state.status = WorkerStatus::Killed;
            let _ = state.save(&state_path);
        }

        self.refresh_workers();
        self.toast(&format!("stopped {}", task.id));
        Ok(())
    }

    /// Suspend TUI and show diff in $PAGER for the selected task.
    pub(super) fn action_review(&mut self) -> TuiResult<()> {
        let Some(task) = self.selected_task() else {
            self.toast("review: no task selected");
            return Ok(());
        };
        let worktree_mgr =
            WorktreeManager::new(self.store.root().clone(), self.config.spawn.clone());
        match worktree_mgr.find(&task.id) {
            Ok(Some(_)) => {
                self.pending_action = TuiAction::SuspendForReview(task.id);
            }
            Ok(None) => {
                self.toast(&format!("review: no branch for {}", task.id));
            }
            Err(e) => {
                self.toast(&format!("review failed: {e}"));
            }
        }
        Ok(())
    }

    /// Reject the selected task with feedback.
    pub(super) fn action_reject(&mut self, feedback: &str) -> TuiResult<()> {
        let Some(task) = self.selected_task() else {
            self.toast("reject: no task selected");
            return Ok(());
        };
        let mut tasks = self.tasks.clone();
        if let Some(target) = tasks.iter_mut().find(|t| t.id == task.id) {
            if !target.notes.is_empty() {
                target.notes.push('\n');
            }
            target.notes.push_str(&format!("REJECTED: {feedback}"));
            target.status = StatusId("todo".into());
        }
        self.store.save_tasks(&tasks)?;
        self.reload_tasks()?;
        self.toast(&format!("rejected {} -- feedback saved", task.id));
        Ok(())
    }

    /// Merge the selected task's worktree branch.
    pub(super) fn action_merge(&mut self) -> TuiResult<()> {
        let Some(task) = self.selected_task() else {
            self.toast("merge: no task selected");
            return Ok(());
        };
        let worktree_mgr =
            WorktreeManager::new(self.store.root().clone(), self.config.spawn.clone());
        let task_title = task.title.clone();

        match merge_worktree(&worktree_mgr, &task.id, &task_title) {
            Ok(MergeResult::Success) => {
                let mut tasks = self.tasks.clone();
                if let Some(target) = tasks.iter_mut().find(|t| t.id == task.id) {
                    target.status = StatusId("done".into());
                }
                self.store.save_tasks(&tasks)?;
                self.reload_tasks()?;
                self.refresh_workers();
                self.toast(&format!("{} merged successfully", task.id));
            }
            Ok(MergeResult::Conflict { details }) => {
                self.toast(&format!("{} merge conflict: {}", task.id, details));
            }
            Err(e) => {
                self.toast(&format!("merge failed: {e}"));
            }
        }
        Ok(())
    }

    pub(super) fn open_create_task_form(&mut self) {
        let epic = if self.current_epic() == ALL_EPIC {
            "default".to_string()
        } else {
            self.current_epic().to_string()
        };
        self.create_task_form = Some(CreateTaskForm::new(epic));
        self.screen = AppScreen::CreateTask;
    }

    pub(super) fn submit_create_task(&mut self) -> TuiResult<()> {
        let Some(form) = self.create_task_form.take() else {
            self.screen = AppScreen::Main;
            return Ok(());
        };

        let title = form.title.text_single_line();
        let title = title.trim();
        if title.is_empty() {
            self.create_task_form = Some(form);
            self.toast("title is required");
            return Ok(());
        }

        let epic = {
            let t = form.epic.text_single_line();
            let t = t.trim();
            if t.is_empty() {
                "default".to_string()
            } else {
                t.to_string()
            }
        };

        let id = self.store.next_task_id(&self.tasks);
        let task = Task {
            id: id.clone(),
            title: title.to_string(),
            epic,
            status: StatusId("todo".into()),
            priority: form.priority,
            depends_on: form.depends_on,
            files: form.files,
            pack_exclude: vec![],
            notes: form.notes.text(),
            acceptance_criteria: form.acceptance_criteria,
            assignee: form.assignee,
            created_at: chrono::Utc::now(),
        };

        let mut new_tasks = self.tasks.clone();
        new_tasks.push(task);
        self.store.save_tasks(&new_tasks)?;
        self.reload_tasks()?;
        self.screen = AppScreen::Main;
        self.toast(&format!("added {}", id.0));
        Ok(())
    }
}

/// Resolve `"all"` to the first available executor by checking PATH for each
/// candidate binary. Falls back to `"claude"` when nothing is found so the
/// caller can surface a clear "not_found" error downstream.
pub(super) fn resolve_executor_name(name: &str, cfg: &TcConfig) -> String {
    if name != "all" {
        return name.to_string();
    }
    let candidates: [(&str, String); 5] = [
        ("claude", "claude".to_string()),
        ("opencode", "opencode".to_string()),
        (
            "codex",
            cfg.executor
                .resolver
                .backends
                .codex
                .as_ref()
                .map(|c| c.command.clone())
                .unwrap_or_else(|| "codex".to_string()),
        ),
        (
            "pi",
            cfg.executor
                .resolver
                .backends
                .pi
                .as_ref()
                .map(|c| c.command.clone())
                .unwrap_or_else(|| "pi".to_string()),
        ),
        (
            "gemini",
            cfg.executor
                .resolver
                .backends
                .gemini
                .as_ref()
                .map(|c| c.command.clone())
                .unwrap_or_else(|| "gemini".to_string()),
        ),
    ];
    'scan: for (alias, cmd) in &candidates {
        if which::which(cmd).is_ok() {
            return (*alias).to_string();
        }
        continue 'scan;
    }
    "claude".to_string()
}
