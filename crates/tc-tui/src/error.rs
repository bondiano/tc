use miette::Diagnostic;
use thiserror::Error;

/// Result alias for TUI operations.
pub type TuiResult<T> = Result<T, TuiError>;

/// Errors from the terminal UI.
#[derive(Debug, Error, Diagnostic)]
pub enum TuiError {
    #[error("terminal I/O error: {0}")]
    #[diagnostic(code(tc::tui::io))]
    Io(#[from] std::io::Error),

    #[error("failed to render: {0}")]
    #[diagnostic(code(tc::tui::render))]
    Render(String),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Storage(#[from] tc_storage::StorageError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Core(#[from] tc_core::error::CoreError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Spawn(#[from] tc_spawn::error::SpawnError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Executor(#[from] tc_executor::error::ExecutorError),
}
