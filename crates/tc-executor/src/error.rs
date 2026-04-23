use miette::Diagnostic;
use thiserror::Error;

/// Result alias for executor operations.
pub type ExecutorResult<T> = Result<T, ExecutorError>;

/// Errors from agent execution (claude, opencode, sandbox).
#[derive(Debug, Error, Diagnostic)]
pub enum ExecutorError {
    #[error("executor '{name}' not found in PATH")]
    #[diagnostic(
        code(tc::executor::not_found),
        help("install '{name}' or add its directory to your PATH")
    )]
    NotFound { name: String },

    #[error("failed to spawn '{command}': {source}")]
    #[diagnostic(
        code(tc::executor::spawn),
        help("check that the command is installed and has execute permissions")
    )]
    SpawnFailed {
        command: String,
        source: std::io::Error,
    },

    #[error("executor '{command}' exited with code {code}")]
    #[diagnostic(
        code(tc::executor::exit),
        help("check the executor logs with `tc logs`")
    )]
    NonZeroExit { command: String, code: i32 },

    #[error("failed to write log to '{path}': {source}")]
    #[diagnostic(
        code(tc::executor::log_write),
        help("check directory permissions and available disk space")
    )]
    LogWrite {
        path: std::path::PathBuf,
        source: std::io::Error,
    },

    #[error("sandbox error: {message}")]
    #[diagnostic(code(tc::executor::sandbox))]
    Sandbox { message: String },

    #[error("nono not found")]
    #[diagnostic(
        code(tc::executor::nono),
        help("install nono for kernel-level sandboxing, or pass --no-sandbox to skip")
    )]
    NonoNotFound,

    #[error("task '{task}' is not ready for execution: {reason}")]
    #[diagnostic(
        code(tc::executor::not_ready),
        help("check task dependencies with `tc show {task}`")
    )]
    TaskNotReady { task: String, reason: String },

    #[error("execution timed out after {seconds}s")]
    #[diagnostic(
        code(tc::executor::timeout),
        help("increase the timeout in .tc/config.yaml or break the task into smaller pieces")
    )]
    Timeout { seconds: u64 },
}

impl ExecutorError {
    pub fn not_found(name: impl Into<String>) -> Self {
        Self::NotFound { name: name.into() }
    }

    pub fn spawn_failed(command: impl Into<String>, source: std::io::Error) -> Self {
        Self::SpawnFailed {
            command: command.into(),
            source,
        }
    }

    pub fn non_zero_exit(command: impl Into<String>, code: i32) -> Self {
        Self::NonZeroExit {
            command: command.into(),
            code,
        }
    }

    pub fn log_write(path: impl Into<std::path::PathBuf>, source: std::io::Error) -> Self {
        Self::LogWrite {
            path: path.into(),
            source,
        }
    }

    pub fn sandbox(message: impl Into<String>) -> Self {
        Self::Sandbox {
            message: message.into(),
        }
    }

    pub fn task_not_ready(task: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::TaskNotReady {
            task: task.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_display() {
        let e = ExecutorError::not_found("claude");
        assert_eq!(e.to_string(), "executor 'claude' not found in PATH");
    }

    #[test]
    fn spawn_failed_display() {
        let e = ExecutorError::spawn_failed(
            "claude",
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
        );
        assert!(e.to_string().starts_with("failed to spawn 'claude'"));
    }

    #[test]
    fn non_zero_exit_display() {
        let e = ExecutorError::non_zero_exit("claude", 1);
        assert_eq!(e.to_string(), "executor 'claude' exited with code 1");
    }

    #[test]
    fn nono_not_found_display() {
        let e = ExecutorError::NonoNotFound;
        assert_eq!(e.to_string(), "nono not found");
    }

    #[test]
    fn task_not_ready_display() {
        let e = ExecutorError::task_not_ready("T-003", "dependency T-001 not done");
        assert_eq!(
            e.to_string(),
            "task 'T-003' is not ready for execution: dependency T-001 not done"
        );
    }

    #[test]
    fn timeout_display() {
        let e = ExecutorError::Timeout { seconds: 300 };
        assert_eq!(e.to_string(), "execution timed out after 300s");
    }

    #[test]
    fn diagnostic_help_on_not_found() {
        use miette::Diagnostic;
        let e = ExecutorError::not_found("claude");
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("PATH"));
    }

    #[test]
    fn diagnostic_help_on_nono() {
        use miette::Diagnostic;
        let e = ExecutorError::NonoNotFound;
        let help = e.help().unwrap().to_string();
        assert!(help.contains("--no-sandbox"));
    }
}
