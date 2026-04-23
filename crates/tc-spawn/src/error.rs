use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// Result alias for spawn operations.
pub type SpawnResult<T> = Result<T, SpawnError>;

/// Errors from parallel agent orchestration (worktrees, process management).
#[derive(Debug, Error, Diagnostic)]
pub enum SpawnError {
    #[error("failed to create worktree at '{path}': {message}")]
    #[diagnostic(
        code(tc::spawn::worktree_create),
        help(
            "ensure the branch doesn't already exist -- try `git branch -D tc/<task>` to clean up"
        )
    )]
    WorktreeCreate { path: PathBuf, message: String },

    #[error("failed to remove worktree at '{path}': {message}")]
    #[diagnostic(
        code(tc::spawn::worktree_remove),
        help("manually remove the directory and run `git worktree prune`")
    )]
    WorktreeRemove { path: PathBuf, message: String },

    #[error("worktree for task '{task}' not found")]
    #[diagnostic(
        code(tc::spawn::worktree_missing),
        help("the worktree may have been cleaned up -- re-run `tc spawn` to recreate it")
    )]
    WorktreeNotFound { task: String },

    #[error("worktree for task '{task}' already exists at '{path}'")]
    #[diagnostic(
        code(tc::spawn::worktree_exists),
        help("remove the existing worktree with `tc kill {task}` first")
    )]
    WorktreeExists { task: String, path: PathBuf },

    #[error("git command failed: {command}: {message}")]
    #[diagnostic(code(tc::spawn::git))]
    Git { command: String, message: String },

    #[error("merge conflict on branch '{branch}': resolve manually")]
    #[diagnostic(
        code(tc::spawn::merge_conflict),
        help("resolve conflicts in the worktree, then run `tc merge` again")
    )]
    MergeConflict { branch: String },

    #[error("failed to spawn worker for task '{task}': {source}")]
    #[diagnostic(
        code(tc::spawn::worker),
        help("check system resources and process limits (`ulimit -u`)")
    )]
    WorkerSpawn {
        task: String,
        #[source]
        source: std::io::Error,
    },

    #[error("worker for task '{task}' was killed")]
    #[diagnostic(
        code(tc::spawn::killed),
        help("the worker may have been OOM-killed -- check system logs (`dmesg` or Console.app)")
    )]
    WorkerKilled { task: String },

    #[error("no ready tasks to spawn")]
    #[diagnostic(
        code(tc::spawn::no_ready),
        help("check task statuses with `tc list` -- tasks may be blocked or already done")
    )]
    NoReadyTasks,

    #[error("file conflict: tasks {tasks} both modify '{path}'")]
    #[diagnostic(
        code(tc::spawn::file_conflict),
        help("split the conflicting files across tasks, or run the tasks sequentially")
    )]
    FileConflict { tasks: String, path: String },

    #[error("I/O error: {message}: {source}")]
    #[diagnostic(code(tc::spawn::io))]
    Io {
        message: String,
        source: std::io::Error,
    },

    #[error("resolver backend '{backend}' timed out after {secs}s")]
    #[diagnostic(
        code(tc::spawn::resolver_timeout),
        help("raise `executor.resolver.timeout_secs` in .tc/config.yaml or resolve manually")
    )]
    ResolverTimeout { backend: String, secs: u64 },

    #[error("resolver template error ({backend}): {message}")]
    #[diagnostic(
        code(tc::spawn::resolver_template),
        help("fix the jinja template at `executor.resolver.template` in .tc/config.yaml")
    )]
    ResolverTemplate { backend: String, message: String },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Executor(#[from] tc_executor::error::ExecutorError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Storage(#[from] tc_storage::StorageError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Core(#[from] tc_core::error::CoreError),
}

impl SpawnError {
    pub fn worktree_create(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::WorktreeCreate {
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn worktree_remove(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::WorktreeRemove {
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn git(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Git {
            command: command.into(),
            message: message.into(),
        }
    }

    pub fn merge_conflict(branch: impl Into<String>) -> Self {
        Self::MergeConflict {
            branch: branch.into(),
        }
    }

    pub fn worker_spawn(task: impl Into<String>, source: std::io::Error) -> Self {
        Self::WorkerSpawn {
            task: task.into(),
            source,
        }
    }

    pub fn file_conflict(tasks: &[String], path: impl Into<String>) -> Self {
        Self::FileConflict {
            tasks: tasks.join(", "),
            path: path.into(),
        }
    }

    pub fn io(message: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            message: message.into(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_create_display() {
        let e =
            SpawnError::worktree_create("/project/.tc-worktrees/T-001", "branch already exists");
        assert!(e.to_string().contains("failed to create worktree"));
        assert!(e.to_string().contains("branch already exists"));
    }

    #[test]
    fn merge_conflict_display() {
        let e = SpawnError::merge_conflict("tc/T-002");
        assert_eq!(
            e.to_string(),
            "merge conflict on branch 'tc/T-002': resolve manually"
        );
    }

    #[test]
    fn file_conflict_display() {
        let e = SpawnError::file_conflict(&["T-001".into(), "T-002".into()], "src/db/schema.rs");
        assert_eq!(
            e.to_string(),
            "file conflict: tasks T-001, T-002 both modify 'src/db/schema.rs'"
        );
    }

    #[test]
    fn git_error_display() {
        let e = SpawnError::git("git worktree add", "fatal: branch already exists");
        assert_eq!(
            e.to_string(),
            "git command failed: git worktree add: fatal: branch already exists"
        );
    }

    #[test]
    fn diagnostic_help_on_file_conflict() {
        use miette::Diagnostic;
        let e = SpawnError::file_conflict(&["T-001".into(), "T-002".into()], "src/main.rs");
        assert!(e.help().is_some());
        assert!(e.help().unwrap().to_string().contains("sequentially"));
    }

    #[test]
    fn diagnostic_transparent_forwards_inner() {
        use miette::Diagnostic;
        let inner = tc_core::error::CoreError::TaskNotFound("T-001".into());
        let e = SpawnError::from(inner);
        // transparent should forward the inner error's diagnostic
        assert!(e.code().is_some());
        assert_eq!(e.code().unwrap().to_string(), "tc::core::not_found");
    }
}
