use miette::Diagnostic;
use thiserror::Error;

/// Result alias for core operations.
pub type CoreResult<T> = Result<T, CoreError>;

/// Errors from the pure domain layer (DAG, status, config, templates).
/// No I/O errors here -- core is side-effect-free.
#[derive(Debug, Error, Diagnostic)]
pub enum CoreError {
    // -- DAG errors --
    #[error("cycle detected in DAG at task '{task}'")]
    #[diagnostic(
        code(tc::core::cycle),
        help("visualise the graph with `tc graph` and remove the circular dependency")
    )]
    CycleDetected { task: String },

    #[error("task '{task}' depends on '{dependency}' which does not exist")]
    #[diagnostic(
        code(tc::core::orphan_dep),
        help(
            "verify the dependency exists with `tc show {dependency}`, or remove it from depends_on"
        )
    )]
    OrphanDependency { task: String, dependency: String },

    #[error("duplicate task ID '{0}'")]
    #[diagnostic(
        code(tc::core::duplicate_id),
        help("use `tc list` to see existing task IDs and pick a unique one")
    )]
    DuplicateTaskId(String),

    // -- Status errors --
    #[error("unknown status '{got}' (valid: {valid})")]
    #[diagnostic(
        code(tc::core::unknown_status),
        help("use one of the valid statuses listed above")
    )]
    UnknownStatus { got: String, valid: String },

    #[error("task '{task}' is already in terminal status '{status}'")]
    #[diagnostic(
        code(tc::core::already_terminal),
        help("use `tc status {task} todo` to reopen the task first")
    )]
    AlreadyTerminal { task: String, status: String },

    #[error("task '{task}' has unresolved dependencies: {unresolved}")]
    #[diagnostic(
        code(tc::core::unresolved_deps),
        help("complete or remove the listed dependencies before proceeding")
    )]
    UnresolvedDependencies { task: String, unresolved: String },

    // -- Task errors --
    #[error("task '{0}' not found")]
    #[diagnostic(
        code(tc::core::not_found),
        help("run `tc list` to see all available tasks")
    )]
    TaskNotFound(String),

    #[error("invalid task ID format '{id}' (expected T-NNN)")]
    #[diagnostic(
        code(tc::core::invalid_id),
        help("task IDs must match the pattern T-NNN (e.g. T-001, T-042)")
    )]
    InvalidTaskId { id: String },

    // -- Template errors --
    #[error("template error: {message}")]
    #[diagnostic(code(tc::core::template))]
    Template { message: String },

    // -- Config errors --
    #[error("invalid config: {field}: {message}")]
    #[diagnostic(
        code(tc::core::config),
        help("check .tc/config.yaml for the correct format")
    )]
    InvalidConfig { field: String, message: String },

    // -- Validation aggregation --
    #[error(
        "validation failed with {count} error(s):\n{}",
        errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")
    )]
    #[diagnostic(code(tc::core::validation))]
    ValidationErrors {
        count: usize,
        errors: Vec<CoreError>,
    },
}

impl CoreError {
    // -- Helper constructors for ergonomic error creation --

    pub fn cycle(task: impl Into<String>) -> Self {
        Self::CycleDetected { task: task.into() }
    }

    pub fn orphan_dep(task: impl Into<String>, dependency: impl Into<String>) -> Self {
        Self::OrphanDependency {
            task: task.into(),
            dependency: dependency.into(),
        }
    }

    pub fn unknown_status(got: impl Into<String>, valid_statuses: &[String]) -> Self {
        Self::UnknownStatus {
            got: got.into(),
            valid: valid_statuses.join(", "),
        }
    }

    pub fn unresolved_deps(task: impl Into<String>, deps: &[String]) -> Self {
        Self::UnresolvedDependencies {
            task: task.into(),
            unresolved: deps.join(", "),
        }
    }

    pub fn template(err: impl std::fmt::Display) -> Self {
        Self::Template {
            message: err.to_string(),
        }
    }

    pub fn invalid_config(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidConfig {
            field: field.into(),
            message: message.into(),
        }
    }

    pub fn validation(errors: Vec<CoreError>) -> Self {
        Self::ValidationErrors {
            count: errors.len(),
            errors,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_error_display() {
        let e = CoreError::cycle("T-001");
        assert_eq!(e.to_string(), "cycle detected in DAG at task 'T-001'");
    }

    #[test]
    fn orphan_dep_display() {
        let e = CoreError::orphan_dep("T-002", "T-999");
        assert_eq!(
            e.to_string(),
            "task 'T-002' depends on 'T-999' which does not exist"
        );
    }

    #[test]
    fn unknown_status_display() {
        let e = CoreError::unknown_status("wip", &["todo".into(), "done".into(), "blocked".into()]);
        assert_eq!(
            e.to_string(),
            "unknown status 'wip' (valid: todo, done, blocked)"
        );
    }

    #[test]
    fn unresolved_deps_display() {
        let e = CoreError::unresolved_deps("T-003", &["T-001".into(), "T-002".into()]);
        assert_eq!(
            e.to_string(),
            "task 'T-003' has unresolved dependencies: T-001, T-002"
        );
    }

    #[test]
    fn template_error_display() {
        let e = CoreError::template("undefined variable 'foo'");
        assert_eq!(e.to_string(), "template error: undefined variable 'foo'");
    }

    #[test]
    fn invalid_config_display() {
        let e = CoreError::invalid_config("spawn.max_parallel", "must be > 0");
        assert_eq!(
            e.to_string(),
            "invalid config: spawn.max_parallel: must be > 0"
        );
    }

    #[test]
    fn validation_errors_display() {
        let errors = vec![
            CoreError::cycle("T-001"),
            CoreError::orphan_dep("T-002", "T-999"),
        ];
        let e = CoreError::validation(errors);
        let msg = e.to_string();
        assert!(msg.starts_with("validation failed with 2 error(s):"));
        assert!(msg.contains("T-001"));
        assert!(msg.contains("T-999"));
    }

    #[test]
    fn task_not_found_display() {
        let e = CoreError::TaskNotFound("T-042".into());
        assert_eq!(e.to_string(), "task 'T-042' not found");
    }

    #[test]
    fn already_terminal_display() {
        let e = CoreError::AlreadyTerminal {
            task: "T-001".into(),
            status: "done".into(),
        };
        assert_eq!(
            e.to_string(),
            "task 'T-001' is already in terminal status 'done'"
        );
    }

    #[test]
    fn diagnostic_help_present() {
        use miette::Diagnostic;
        let e = CoreError::TaskNotFound("T-042".into());
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("tc list"));
    }

    #[test]
    fn diagnostic_code_present() {
        use miette::Diagnostic;
        let e = CoreError::cycle("T-001");
        let code = e.code().unwrap().to_string();
        assert_eq!(code, "tc::core::cycle");
    }
}
