use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// Result alias for packer operations.
pub type PackerResult<T> = Result<T, PackerError>;

/// Errors from codebase packing.
#[derive(Debug, Error, Diagnostic)]
pub enum PackerError {
    #[error("failed to read '{path}': {source}")]
    #[diagnostic(
        code(tc::packer::read),
        help("check that the file exists and is readable")
    )]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to walk directory '{root}': {message}")]
    #[diagnostic(
        code(tc::packer::walk),
        help("check that the directory exists and is accessible")
    )]
    Walk { root: PathBuf, message: String },

    #[error("invalid glob pattern '{pattern}': {message}")]
    #[diagnostic(
        code(tc::packer::glob),
        help("glob syntax: * (one segment), ** (recursive), ? (single char), [abc] (char class)")
    )]
    InvalidGlob { pattern: String, message: String },

    #[error("token budget exceeded: {used} tokens > {budget} budget")]
    #[diagnostic(
        code(tc::packer::budget),
        help(
            "narrow scope with pack_exclude in the task, or increase token_budget in .tc/config.yaml"
        )
    )]
    BudgetExceeded { used: usize, budget: usize },

    #[error("potential secret detected in '{file}': {description}")]
    #[diagnostic(
        code(tc::packer::secret),
        help(
            "add the file to pack_exclude in the task definition, or remove the secret from the file"
        )
    )]
    SecretDetected { file: String, description: String },

    #[error("no files matched the given paths")]
    #[diagnostic(
        code(tc::packer::no_match),
        help("check that the file paths exist relative to the project root")
    )]
    NoFilesMatched,
}

impl PackerError {
    pub fn file_read(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::FileRead {
            path: path.into(),
            source,
        }
    }

    pub fn walk(root: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Walk {
            root: root.into(),
            message: message.into(),
        }
    }

    pub fn invalid_glob(pattern: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidGlob {
            pattern: pattern.into(),
            message: message.into(),
        }
    }

    pub fn secret(file: impl Into<String>, description: impl Into<String>) -> Self {
        Self::SecretDetected {
            file: file.into(),
            description: description.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_read_display() {
        let e = PackerError::file_read(
            "/some/path.rs",
            std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        );
        assert!(e.to_string().starts_with("failed to read '/some/path.rs'"));
    }

    #[test]
    fn budget_exceeded_display() {
        let e = PackerError::BudgetExceeded {
            used: 100_000,
            budget: 80_000,
        };
        assert_eq!(
            e.to_string(),
            "token budget exceeded: 100000 tokens > 80000 budget"
        );
    }

    #[test]
    fn invalid_glob_display() {
        let e = PackerError::invalid_glob("**[", "unclosed bracket");
        assert_eq!(
            e.to_string(),
            "invalid glob pattern '**[': unclosed bracket"
        );
    }

    #[test]
    fn secret_detected_display() {
        let e = PackerError::secret("config.yaml", "AWS access key");
        assert_eq!(
            e.to_string(),
            "potential secret detected in 'config.yaml': AWS access key"
        );
    }

    #[test]
    fn no_files_matched_display() {
        let e = PackerError::NoFilesMatched;
        assert_eq!(e.to_string(), "no files matched the given paths");
    }

    #[test]
    fn diagnostic_help_on_budget() {
        use miette::Diagnostic;
        let e = PackerError::BudgetExceeded {
            used: 100_000,
            budget: 80_000,
        };
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("pack_exclude"));
    }

    #[test]
    fn diagnostic_help_on_secret() {
        use miette::Diagnostic;
        let e = PackerError::secret("config.yaml", "AWS access key");
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("pack_exclude"));
    }
}
