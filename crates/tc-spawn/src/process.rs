use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tc_core::task::TaskId;
use tc_executor::traits::ExecutionResult;

use crate::error::SpawnError;
use crate::tmux;

/// Persistent worker state written to `.tc/workers/{task_id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    pub task_id: String,
    pub pid: u32,
    pub started_at: DateTime<Utc>,
    pub worktree_path: String,
    pub status: WorkerStatus,
    pub log_path: String,
    /// Tmux session name, if the worker was spawned inside tmux.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Running,
    Completed,
    Failed,
    Killed,
}

impl std::fmt::Display for WorkerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Killed => write!(f, "killed"),
        }
    }
}

impl WorkerState {
    pub fn new(
        task_id: &TaskId,
        pid: u32,
        worktree_path: &std::path::Path,
        log_path: &std::path::Path,
    ) -> Self {
        Self {
            task_id: task_id.0.clone(),
            pid,
            started_at: Utc::now(),
            worktree_path: worktree_path.to_string_lossy().to_string(),
            status: WorkerStatus::Running,
            log_path: log_path.to_string_lossy().to_string(),
            tmux_session: None,
        }
    }

    pub fn new_tmux(
        task_id: &TaskId,
        pid: u32,
        worktree_path: &std::path::Path,
        log_path: &std::path::Path,
        tmux_session: String,
    ) -> Self {
        Self {
            task_id: task_id.0.clone(),
            pid,
            started_at: Utc::now(),
            worktree_path: worktree_path.to_string_lossy().to_string(),
            status: WorkerStatus::Running,
            log_path: log_path.to_string_lossy().to_string(),
            tmux_session: Some(tmux_session),
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), SpawnError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SpawnError::io("create workers dir", e))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SpawnError::io("serialize worker state", std::io::Error::other(e)))?;
        std::fs::write(path, json).map_err(|e| SpawnError::io("write worker state", e))?;
        Ok(())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, SpawnError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| SpawnError::io("read worker state", e))?;
        serde_json::from_str(&content).map_err(|e| {
            SpawnError::io(
                "parse worker state",
                std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            )
        })
    }
}

pub enum WorkerHandle {
    /// Worker spawned as a direct child process.
    Process {
        task_id: TaskId,
        log_path: PathBuf,
        child: tokio::process::Child,
    },
    /// Worker running inside a tmux session.
    Tmux {
        task_id: TaskId,
        log_path: PathBuf,
        session_name: String,
        exit_code_path: PathBuf,
    },
}

impl WorkerHandle {
    pub fn new(task_id: TaskId, log_path: PathBuf, child: tokio::process::Child) -> Self {
        Self::Process {
            task_id,
            log_path,
            child,
        }
    }

    pub fn new_tmux(
        task_id: TaskId,
        log_path: PathBuf,
        session_name: String,
        exit_code_path: PathBuf,
    ) -> Self {
        Self::Tmux {
            task_id,
            log_path,
            session_name,
            exit_code_path,
        }
    }

    pub fn task_id(&self) -> &TaskId {
        match self {
            Self::Process { task_id, .. } | Self::Tmux { task_id, .. } => task_id,
        }
    }

    pub fn log_path(&self) -> &PathBuf {
        match self {
            Self::Process { log_path, .. } | Self::Tmux { log_path, .. } => log_path,
        }
    }

    pub fn pid(&self) -> Option<u32> {
        match self {
            Self::Process { child, .. } => child.id(),
            Self::Tmux { session_name, .. } => tmux::session_pid(session_name),
        }
    }

    pub async fn wait(&mut self) -> Result<ExecutionResult, SpawnError> {
        match self {
            Self::Process {
                task_id,
                log_path,
                child,
            } => {
                let status = child
                    .wait()
                    .await
                    .map_err(|e| SpawnError::worker_spawn(&task_id.0, e))?;
                Ok(ExecutionResult {
                    exit_code: status.code().unwrap_or(-1),
                    log_path: Some(log_path.clone()),
                })
            }
            Self::Tmux {
                log_path,
                session_name,
                exit_code_path,
                ..
            } => {
                // Poll until session is gone
                while tmux::has_session(session_name) {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                let exit_code = tmux::read_exit_code(exit_code_path).unwrap_or(-1);
                Ok(ExecutionResult {
                    exit_code,
                    log_path: Some(log_path.clone()),
                })
            }
        }
    }

    pub fn kill(&mut self) -> Result<(), SpawnError> {
        match self {
            Self::Process { task_id, child, .. } => {
                child
                    .start_kill()
                    .map_err(|e| SpawnError::worker_spawn(&task_id.0, e))?;
            }
            Self::Tmux { session_name, .. } => {
                let _ = tmux::kill_session(session_name);
            }
        }
        Ok(())
    }

    pub fn is_running(&mut self) -> bool {
        match self {
            Self::Process { child, .. } => child.try_wait().ok().flatten().is_none(),
            Self::Tmux { session_name, .. } => tmux::has_session(session_name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn worker_state_save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("workers/T-001.json");

        let state = WorkerState::new(
            &TaskId("T-001".into()),
            12345,
            std::path::Path::new("/project/.tc-worktrees/T-001"),
            std::path::Path::new("/project/.tc/logs/T-001.log"),
        );

        state.save(&path).unwrap();
        let loaded = WorkerState::load(&path).unwrap();

        assert_eq!(loaded.task_id, "T-001");
        assert_eq!(loaded.pid, 12345);
        assert_eq!(loaded.status, WorkerStatus::Running);
        assert_eq!(loaded.tmux_session, None);
    }

    #[test]
    fn worker_state_tmux_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("workers/T-002.json");

        let state = WorkerState::new_tmux(
            &TaskId("T-002".into()),
            12345,
            std::path::Path::new("/project/.tc-worktrees/T-002"),
            std::path::Path::new("/project/.tc/logs/T-002.log"),
            "tc-T-002".into(),
        );

        state.save(&path).unwrap();
        let loaded = WorkerState::load(&path).unwrap();

        assert_eq!(loaded.task_id, "T-002");
        assert_eq!(loaded.tmux_session, Some("tc-T-002".into()));
    }

    #[test]
    fn worker_state_legacy_json_missing_tmux_field() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("workers/T-003.json");

        // Simulate a legacy JSON without tmux_session
        let json = r#"{"task_id":"T-003","pid":1,"started_at":"2025-01-01T00:00:00Z","worktree_path":"/tmp","status":"running","log_path":"/tmp/log"}"#;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, json).unwrap();

        let loaded = WorkerState::load(&path).unwrap();
        assert_eq!(loaded.tmux_session, None);
    }

    #[test]
    fn worker_status_display() {
        assert_eq!(WorkerStatus::Running.to_string(), "running");
        assert_eq!(WorkerStatus::Completed.to_string(), "completed");
        assert_eq!(WorkerStatus::Failed.to_string(), "failed");
        assert_eq!(WorkerStatus::Killed.to_string(), "killed");
    }

    #[test]
    fn worker_state_load_missing_file() {
        let err = WorkerState::load(std::path::Path::new("/nonexistent/state.json")).unwrap_err();
        assert!(err.to_string().contains("read worker state"));
    }
}
