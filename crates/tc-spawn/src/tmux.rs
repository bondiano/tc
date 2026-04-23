use std::path::Path;
use std::process::Command;

use crate::error::SpawnError;

const SESSION_PREFIX: &str = "tc-";

/// Build the canonical tmux session name for a task.
pub fn session_name(task_id: &str) -> String {
    format!("{SESSION_PREFIX}{task_id}")
}

/// Check whether tmux is installed and available in PATH.
pub fn is_available() -> bool {
    which::which("tmux").is_ok()
}

/// Check whether a tmux session with the given name exists.
pub fn has_session(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Create a detached tmux session that runs `shell_command` in `working_dir`.
///
/// The session is created with `remain-on-exit off` so it auto-destroys when
/// the command finishes. An exit-code file is written so the scheduler can
/// retrieve the result after the session is gone.
///
/// Logging is set up via `pipe-pane` so all terminal output is also captured
/// to `log_path`.
pub fn create_session(
    name: &str,
    shell_command: &str,
    working_dir: &Path,
    log_path: &Path,
    exit_code_path: &Path,
) -> Result<(), SpawnError> {
    // Build the wrapper that runs the real command, captures exit code, then exits.
    let wrapper = format!(
        "{shell_command}\nEC=$?\necho $EC > {exit_path}\nexit $EC",
        shell_command = shell_command,
        exit_path = shell_escape(exit_code_path.to_string_lossy().as_ref()),
    );

    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            name,
            "-c",
            &working_dir.to_string_lossy(),
            "bash",
            "-c",
            &wrapper,
        ])
        .status()
        .map_err(|e| SpawnError::io("tmux new-session", e))?;

    if !status.success() {
        return Err(SpawnError::io(
            "tmux new-session",
            std::io::Error::other(format!("tmux new-session exited with {:?}", status.code())),
        ));
    }

    // Set up logging: pipe all pane output to the log file.
    let pipe_cmd = format!(
        "cat >> {}",
        shell_escape(log_path.to_string_lossy().as_ref())
    );
    let _ = Command::new("tmux")
        .args(["pipe-pane", "-t", name, "-o", &pipe_cmd])
        .status();

    Ok(())
}

/// Attach the current terminal to a tmux session (replaces the process).
pub fn attach_session(name: &str) -> Result<(), SpawnError> {
    let err = exec_tmux_attach(name);
    Err(SpawnError::io("tmux attach", err))
}

/// Kill a tmux session by name.
pub fn kill_session(name: &str) -> Result<(), SpawnError> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| SpawnError::io("tmux kill-session", e))?;

    if !status.success() {
        return Err(SpawnError::io(
            "tmux kill-session",
            std::io::Error::other(format!("tmux kill-session exited with {:?}", status.code())),
        ));
    }
    Ok(())
}

/// Read the exit code from the file written by the tmux wrapper.
///
/// Returns `None` if the file doesn't exist yet (command still running or
/// crashed before writing).
pub fn read_exit_code(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Get the PID of the first pane's process in a tmux session.
pub fn session_pid(name: &str) -> Option<u32> {
    let output = Command::new("tmux")
        .args(["list-panes", "-t", name, "-F", "#{pane_pid}"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|line| line.trim().parse().ok())
}

/// Build a shell command string from program + args with proper escaping.
pub fn build_shell_command(program: &str, args: &[String]) -> String {
    let mut parts = vec![shell_escape(program)];
    for arg in args {
        parts.push(shell_escape(arg));
    }
    parts.join(" ")
}

/// Minimal shell escaping: wrap in single quotes, escaping embedded single quotes.
fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // If the string contains no special characters, return as-is
    if s.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | '+'))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Use exec to replace the current process with tmux attach.
/// On Unix, uses execvp so the tc process becomes tmux.
fn exec_tmux_attach(name: &str) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    // This only returns if exec fails
    Command::new("tmux")
        .args(["attach-session", "-t", name])
        .exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_name_format() {
        assert_eq!(session_name("T-001"), "tc-T-001");
        assert_eq!(session_name("T-123"), "tc-T-123");
    }

    #[test]
    fn shell_escape_simple() {
        assert_eq!(shell_escape("hello"), "hello");
        assert_eq!(shell_escape("path/to/file"), "path/to/file");
        assert_eq!(shell_escape("--flag=value"), "--flag=value");
    }

    #[test]
    fn shell_escape_special() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn shell_escape_complex_context() {
        let ctx = "Implement the feature\nWith newlines";
        let escaped = shell_escape(ctx);
        assert!(escaped.starts_with('\''));
        assert!(escaped.ends_with('\''));
    }

    #[test]
    fn build_shell_command_basic() {
        let cmd = build_shell_command("claude", &["--print".into(), "hello world".into()]);
        assert_eq!(cmd, "claude --print 'hello world'");
    }

    #[test]
    fn has_session_returns_false_for_nonexistent() {
        assert!(!has_session("tc-nonexistent-session-xyz-99999"));
    }

    #[test]
    fn read_exit_code_missing_file() {
        assert_eq!(read_exit_code(Path::new("/nonexistent/file")), None);
    }

    #[test]
    fn read_exit_code_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exit_code");
        std::fs::write(&path, "0\n").unwrap();
        assert_eq!(read_exit_code(&path), Some(0));
    }

    #[test]
    fn read_exit_code_nonzero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("exit_code");
        std::fs::write(&path, "1\n").unwrap();
        assert_eq!(read_exit_code(&path), Some(1));
    }
}
