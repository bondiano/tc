pub mod config;
pub mod error;
pub mod init;
pub mod tasks;

use std::path::PathBuf;

pub use error::{StorageError, StorageResult};

use tc_core::config::TcConfig;
use tc_core::task::{Task, TaskId};

#[derive(Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn open(root: PathBuf) -> StorageResult<Self> {
        let tc_dir = root.join(".tc");
        if !tc_dir.exists() {
            return Err(StorageError::NotInitialized(root));
        }
        Ok(Self { root })
    }

    pub fn discover() -> StorageResult<Self> {
        let mut dir =
            std::env::current_dir().map_err(|e| StorageError::file_read(PathBuf::from("."), e))?;
        loop {
            if dir.join(".tc").exists() {
                return Self::open(dir);
            }
            if !dir.pop() {
                return Err(StorageError::NotFound);
            }
        }
    }

    pub fn load_tasks(&self) -> StorageResult<Vec<Task>> {
        tasks::load(&self.tasks_path())
    }

    pub fn save_tasks(&self, tasks: &[Task]) -> StorageResult<()> {
        tasks::save(&self.tasks_path(), tasks)
    }

    pub fn next_task_id(&self, tasks: &[Task]) -> TaskId {
        tasks::next_id(tasks)
    }

    pub fn load_config(&self) -> StorageResult<TcConfig> {
        config::load(&self.config_path())
    }

    pub fn save_config(&self, config: &TcConfig) -> StorageResult<()> {
        config::save(&self.config_path(), config)
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn tc_dir(&self) -> PathBuf {
        self.root.join(".tc")
    }

    pub fn context_path(&self) -> PathBuf {
        self.tc_dir().join("TASK_CONTEXT.md")
    }

    pub fn log_path(&self, id: &TaskId) -> PathBuf {
        self.tc_dir().join("logs").join(format!("{}.log", id.0))
    }

    pub fn workers_dir(&self) -> PathBuf {
        self.tc_dir().join("workers")
    }

    pub fn worker_state_path(&self, id: &TaskId) -> PathBuf {
        self.workers_dir().join(format!("{}.json", id.0))
    }

    pub fn worker_exit_code_path(&self, id: &TaskId) -> PathBuf {
        self.workers_dir().join(format!("{}.exit", id.0))
    }

    fn tasks_path(&self) -> PathBuf {
        self.tc_dir().join("tasks.yaml")
    }

    pub fn config_path(&self) -> PathBuf {
        self.tc_dir().join("config.yaml")
    }

    pub fn draft_add_task_path(&self) -> PathBuf {
        self.tc_dir().join("draft_add_task.txt")
    }
}
