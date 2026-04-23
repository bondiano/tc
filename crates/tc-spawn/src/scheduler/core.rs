use tc_executor::traits::Executor;

use crate::process::WorkerHandle;
use crate::tmux;
use crate::worktree::WorktreeManager;

pub struct Scheduler<E: Executor> {
    pub executor: E,
    pub worktree_mgr: WorktreeManager,
    pub max_parallel: usize,
    pub use_tmux: bool,
    pub(crate) workers: Vec<WorkerHandle>,
}

impl<E: Executor> Scheduler<E> {
    pub fn new(executor: E, worktree_mgr: WorktreeManager, max_parallel: usize) -> Self {
        let use_tmux = tmux::is_available() && std::env::var("TC_NO_TMUX").is_err();
        Self {
            executor,
            worktree_mgr,
            max_parallel,
            use_tmux,
            workers: Vec::new(),
        }
    }

    pub fn active_workers(&self) -> &[WorkerHandle] {
        &self.workers
    }

    pub fn active_count(&self) -> usize {
        self.workers.len()
    }

    /// Validate that a queue of tasks can be safely run together.
    /// Alias for `scheduler::validate_queue`; retained so the scheduler
    /// entry-point is discoverable through the type API.
    pub fn validate_queue(tasks: &[&tc_core::task::Task]) -> Result<(), crate::error::SpawnError> {
        super::validation::validate_queue(tasks)
    }

    /// Detect file conflicts between tasks being spawned.
    /// Alias for `scheduler::detect_file_conflicts`.
    pub fn detect_file_conflicts(
        tasks: &[&tc_core::task::Task],
    ) -> Result<(), crate::error::SpawnError> {
        super::validation::detect_file_conflicts(tasks)
    }
}
