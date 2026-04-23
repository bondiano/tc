use miette::Diagnostic;
use thiserror::Error;

/// Top-level CLI error -- wraps all domain errors with miette for rich output.
/// Uses `#[diagnostic(transparent)]` to forward help hints from inner errors.
#[derive(Debug, Error, Diagnostic)]
pub enum CliError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Core(#[from] tc_core::error::CoreError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Storage(#[from] tc_storage::StorageError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Packer(#[from] tc_packer::PackerError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Executor(#[from] tc_executor::error::ExecutorError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Spawn(#[from] tc_spawn::error::SpawnError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Tui(#[from] tc_tui::error::TuiError),

    #[error("{0}")]
    #[diagnostic(code(tc::user))]
    User(String),
}

impl CliError {
    #[cfg(test)]
    pub fn phase(&self) -> &'static str {
        match self {
            Self::Core(_) => "core",
            Self::Storage(_) => "storage",
            Self::Packer(_) => "packer",
            Self::Executor(_) => "executor",
            Self::Spawn(_) => "spawn",
            Self::Tui(_) => "tui",
            Self::User(_) => "user",
        }
    }

    pub fn user(msg: impl Into<String>) -> Self {
        Self::User(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_tagging() {
        let e = CliError::from(tc_core::error::CoreError::TaskNotFound("T-001".into()));
        assert_eq!(e.phase(), "core");

        let e = CliError::from(tc_storage::StorageError::NotFound);
        assert_eq!(e.phase(), "storage");

        let e = CliError::user("invalid argument");
        assert_eq!(e.phase(), "user");
    }

    #[test]
    fn user_error_display() {
        let e = CliError::user("task ID is required");
        assert_eq!(e.to_string(), "task ID is required");
    }

    #[test]
    fn transparent_forwards_diagnostic() {
        use miette::Diagnostic;
        let e = CliError::from(tc_core::error::CoreError::TaskNotFound("T-001".into()));
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("tc list"));
        assert!(e.code().is_some());
        assert_eq!(e.code().unwrap().to_string(), "tc::core::not_found");
    }

    #[test]
    fn transparent_forwards_storage_diagnostic() {
        use miette::Diagnostic;
        let e = CliError::from(tc_storage::StorageError::NotFound);
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("tc init"));
    }
}
