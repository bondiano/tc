use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// Result alias for storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

/// Errors from YAML persistence layer.
#[derive(Debug, Error, Diagnostic)]
pub enum StorageError {
    #[error("failed to read '{path}': {source}")]
    #[diagnostic(
        code(tc::storage::read),
        help("check that the file exists and has read permissions")
    )]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write '{path}': {source}")]
    #[diagnostic(
        code(tc::storage::write),
        help("check file permissions and available disk space")
    )]
    FileWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to create directory '{path}': {source}")]
    #[diagnostic(code(tc::storage::dir), help("check parent directory permissions"))]
    DirCreate {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse YAML in '{path}': {source}")]
    #[diagnostic(
        code(tc::storage::yaml_parse),
        help("check the YAML syntax -- a common issue is incorrect indentation or missing quotes")
    )]
    YamlParse {
        path: PathBuf,
        source: serde_yaml_ng::Error,
    },

    #[error("failed to serialize YAML: {0}")]
    #[diagnostic(code(tc::storage::yaml_serialize))]
    YamlSerialize(#[source] serde_yaml_ng::Error),

    #[error("project not initialized (no .tc/ found at '{0}')")]
    #[diagnostic(
        code(tc::storage::not_init),
        help("run `tc init` to initialize the project")
    )]
    NotInitialized(PathBuf),

    #[error("no .tc/ directory found in current or parent directories")]
    #[diagnostic(
        code(tc::storage::not_found),
        help("run `tc init` in your project root to create a .tc/ directory")
    )]
    NotFound,

    #[error("project already initialized at '{0}'")]
    #[diagnostic(code(tc::storage::already_init))]
    AlreadyInitialized(PathBuf),

    #[error("config validation failed: {0}")]
    ConfigValidation(#[source] tc_core::error::CoreError),

    #[error("{0}")]
    #[diagnostic(transparent)]
    Core(#[from] tc_core::error::CoreError),

    #[error("timed out acquiring lock on '{path}' after {seconds}s")]
    #[diagnostic(
        code(tc::storage::lock_timeout),
        help("another tc process is holding the lock; retry or check for stuck processes")
    )]
    LockTimeout { path: PathBuf, seconds: u64 },
}

impl StorageError {
    pub fn file_read(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::FileRead {
            path: path.into(),
            source,
        }
    }

    pub fn file_write(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::FileWrite {
            path: path.into(),
            source,
        }
    }

    pub fn dir_create(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::DirCreate {
            path: path.into(),
            source,
        }
    }

    pub fn yaml_parse(path: impl Into<PathBuf>, source: serde_yaml_ng::Error) -> Self {
        Self::YamlParse {
            path: path.into(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_initialized_display() {
        let e = StorageError::NotInitialized(PathBuf::from("/my/project"));
        assert_eq!(
            e.to_string(),
            "project not initialized (no .tc/ found at '/my/project')"
        );
    }

    #[test]
    fn not_found_display() {
        let e = StorageError::NotFound;
        assert_eq!(
            e.to_string(),
            "no .tc/ directory found in current or parent directories"
        );
    }

    #[test]
    fn file_read_display() {
        let e = StorageError::file_read(
            "/my/project/.tc/tasks.yaml",
            std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        );
        assert!(
            e.to_string()
                .starts_with("failed to read '/my/project/.tc/tasks.yaml'")
        );
    }

    #[test]
    fn already_initialized_display() {
        let e = StorageError::AlreadyInitialized(PathBuf::from("/my/project"));
        assert_eq!(
            e.to_string(),
            "project already initialized at '/my/project'"
        );
    }

    #[test]
    fn diagnostic_help_on_not_found() {
        use miette::Diagnostic;
        let e = StorageError::NotFound;
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("tc init"));
    }

    #[test]
    fn diagnostic_help_on_not_initialized() {
        use miette::Diagnostic;
        let e = StorageError::NotInitialized(PathBuf::from("/tmp"));
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("tc init"));
    }
}
