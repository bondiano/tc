use std::path::Path;
use std::process::Stdio;

use crate::error::ExecutorError;

/// Result of running verification commands.
#[derive(Debug)]
pub struct VerifyResult {
    pub passed: bool,
    pub results: Vec<CommandResult>,
}

/// Result of a single verification command.
#[derive(Debug)]
pub struct CommandResult {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Run verification commands sequentially in the given working directory.
///
/// Returns early on the first failure. All commands must pass for `passed` to be true.
pub async fn run_verification(
    commands: &[String],
    working_dir: &Path,
) -> Result<VerifyResult, ExecutorError> {
    let mut results = Vec::new();

    'verify: for cmd_str in commands {
        if cmd_str.trim().is_empty() {
            continue 'verify;
        }

        // Run through a real shell so quoted args, `&&`, pipes, redirects,
        // and globs work like users expect. Split-whitespace parsing here
        // previously broke anything beyond a single bare command.
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd_str)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| ExecutorError::spawn_failed(cmd_str.clone(), e))?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let passed = exit_code == 0;
        results.push(CommandResult {
            command: cmd_str.clone(),
            exit_code,
            stdout,
            stderr,
        });

        if !passed {
            return Ok(VerifyResult {
                passed: false,
                results,
            });
        }
    }

    Ok(VerifyResult {
        passed: true,
        results,
    })
}

/// Format verification failure diagnostics for retry context.
pub fn format_failure_diagnostics(result: &VerifyResult) -> String {
    let mut output = String::from("## Verification Failed\n\n");
    'fmt: for r in &result.results {
        if r.exit_code != 0 {
            output.push_str(&format!("### Command: `{}`\n", r.command));
            output.push_str(&format!("Exit code: {}\n\n", r.exit_code));
            if !r.stderr.is_empty() {
                output.push_str("**stderr:**\n```\n");
                // Limit stderr to last 100 lines
                let lines: Vec<&str> = r.stderr.lines().collect();
                let start = lines.len().saturating_sub(100);
                for line in &lines[start..] {
                    output.push_str(line);
                    output.push('\n');
                }
                output.push_str("```\n\n");
            }
            break 'fmt;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn verify_passing_commands() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(&["true".to_string()], dir.path())
            .await
            .unwrap();
        assert!(result.passed);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].exit_code, 0);
    }

    #[tokio::test]
    async fn verify_failing_command() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(&["false".to_string()], dir.path())
            .await
            .unwrap();
        assert!(!result.passed);
        assert_eq!(result.results.len(), 1);
        assert_ne!(result.results[0].exit_code, 0);
    }

    #[tokio::test]
    async fn verify_stops_on_first_failure() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(
            &[
                "true".to_string(),
                "false".to_string(),
                "true".to_string(), // should not run
            ],
            dir.path(),
        )
        .await
        .unwrap();
        assert!(!result.passed);
        assert_eq!(result.results.len(), 2);
    }

    #[tokio::test]
    async fn verify_multiple_passing() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(
            &["true".to_string(), "true".to_string(), "true".to_string()],
            dir.path(),
        )
        .await
        .unwrap();
        assert!(result.passed);
        assert_eq!(result.results.len(), 3);
    }

    #[tokio::test]
    async fn verify_empty_commands() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(&[], dir.path()).await.unwrap();
        assert!(result.passed);
        assert!(result.results.is_empty());
    }

    #[tokio::test]
    async fn verify_captures_output() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(&["echo hello".to_string()], dir.path())
            .await
            .unwrap();
        assert!(result.passed);
        assert!(result.results[0].stdout.contains("hello"));
    }

    #[tokio::test]
    async fn verify_shell_operators() {
        // `true && false` should fail -- requires shell-level composition.
        let dir = TempDir::new().unwrap();
        let result = run_verification(&["true && false".to_string()], dir.path())
            .await
            .unwrap();
        assert!(!result.passed);
    }

    #[tokio::test]
    async fn verify_quoted_args_with_spaces() {
        // Without sh -c, `echo "hello world"` would pass `"hello`, `world"` as
        // separate tokens and break any tool that cares about quoting.
        let dir = TempDir::new().unwrap();
        let result = run_verification(&[r#"echo "hello world""#.to_string()], dir.path())
            .await
            .unwrap();
        assert!(result.passed);
        assert!(result.results[0].stdout.contains("hello world"));
    }

    #[tokio::test]
    async fn verify_shell_pipe() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(&["echo hi | grep hi".to_string()], dir.path())
            .await
            .unwrap();
        assert!(result.passed);
    }

    #[tokio::test]
    async fn verify_skips_blank_commands() {
        let dir = TempDir::new().unwrap();
        let result = run_verification(
            &["   ".to_string(), "true".to_string(), "".to_string()],
            dir.path(),
        )
        .await
        .unwrap();
        assert!(result.passed);
        // Only the non-empty command should actually run.
        assert_eq!(result.results.len(), 1);
    }

    #[test]
    fn format_diagnostics_shows_failure() {
        let result = VerifyResult {
            passed: false,
            results: vec![
                CommandResult {
                    command: "cargo check".to_string(),
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                },
                CommandResult {
                    command: "cargo test".to_string(),
                    exit_code: 1,
                    stdout: String::new(),
                    stderr: "test foo failed".to_string(),
                },
            ],
        };
        let diag = format_failure_diagnostics(&result);
        assert!(diag.contains("cargo test"));
        assert!(diag.contains("test foo failed"));
        assert!(!diag.contains("cargo check")); // passing command not shown as failure
    }
}
