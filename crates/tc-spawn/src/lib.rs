#[cfg(not(unix))]
compile_error!(
    "tc-spawn currently supports Unix only (macOS, Linux). Windows is not supported; PRs welcome."
);

pub mod error;
pub mod merge;
pub mod process;
pub mod recovery;
pub mod resolver;
pub mod scheduler;
pub mod tmux;
pub mod worktree;
