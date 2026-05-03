use std::path::Path;

use serde::Deserialize;

use crate::error::ExecutorError;
use crate::io::spawn_and_wait;
use crate::traits::{ExecutionMode, ExecutionRequest, ExecutionResult, Executor};

/// Verdict written by the reviewer agent to `.tc/.review_verdict.json` before exit.
///
/// The agent is instructed (via system prompt) to emit JSON-only output of the form:
/// `{"summary":"...","confidence":0.85,"risks":["risk1","risk2"]}`
///
/// `confidence` is a soft probability of the change being safe to merge, in `[0.0, 1.0]`.
/// `risks` enumerates concrete concerns the reviewer flagged; may be empty.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ReviewVerdict {
    pub summary: String,
    pub confidence: f32,
    #[serde(default)]
    pub risks: Vec<String>,
}

impl ReviewVerdict {
    /// Confidence clamped to the `[0.0, 1.0]` range. Models occasionally
    /// return slight overshoots (e.g. `1.01`) -- callers comparing against
    /// thresholds should use this rather than the raw field.
    pub fn confidence_clamped(&self) -> f32 {
        self.confidence.clamp(0.0, 1.0)
    }
}

/// Read and parse a review verdict file.
///
/// Returns `Ok(None)` if the file does not exist (the reviewer did not produce a verdict).
/// Returns `Ok(Some(_))` on successful parse and `Err` on I/O failure or malformed JSON.
pub fn read_review(path: &Path) -> Result<Option<ReviewVerdict>, ExecutorError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ExecutorError::review_read(path, e)),
    };

    let verdict = serde_json::from_slice::<ReviewVerdict>(&bytes)
        .map_err(|e| ExecutorError::review_parse(path, e))?;
    Ok(Some(verdict))
}

/// Executor that runs Claude as a code-review agent.
///
/// The agent receives the diff and task metadata via `request.context` and writes
/// a JSON verdict file consumed by [`read_review`].
pub struct ReviewerExecutor {
    pub system_prompt: String,
}

impl ReviewerExecutor {
    const PROGRAM: &'static str = "claude";

    fn build_args(&self, request: &ExecutionRequest) -> Vec<String> {
        let mut args = Vec::new();

        match request.mode {
            ExecutionMode::Interactive => {}
            ExecutionMode::Accept => {
                args.push("--permission-mode".to_string());
                args.push("acceptEdits".to_string());
            }
            ExecutionMode::Yolo => {
                args.push("--permission-mode".to_string());
                args.push("bypassPermissions".to_string());
                args.push("--print".to_string());
                args.push(request.context.clone());
            }
        }

        if !self.system_prompt.is_empty() {
            args.push("--system-prompt".to_string());
            args.push(self.system_prompt.clone());
        }

        for mcp in &request.mcp_servers {
            args.push("--mcp-server".to_string());
            args.push(format!("{} -- {}", mcp.name, mcp.command));
        }

        args
    }
}

impl Executor for ReviewerExecutor {
    fn build_command(
        &self,
        request: &ExecutionRequest,
    ) -> Result<tokio::process::Command, ExecutorError> {
        if which::which(Self::PROGRAM).is_err() {
            return Err(ExecutorError::not_found(Self::PROGRAM));
        }

        let args = self.build_args(request);

        let mut cmd = tokio::process::Command::new(Self::PROGRAM);
        cmd.args(&args);
        cmd.current_dir(&request.working_dir);

        Ok(cmd)
    }

    async fn execute(
        &self,
        request: &ExecutionRequest,
        log_sink: Option<&Path>,
    ) -> Result<ExecutionResult, ExecutorError> {
        let cmd = self.build_command(request)?;
        spawn_and_wait(cmd, log_sink, Self::PROGRAM).await
    }
}

/// Run a reviewer end-to-end: execute the agent, then read and parse its verdict file.
///
/// `verdict_path` is the location the agent is configured (via system prompt) to write to.
/// A non-zero executor exit produces `ExecutorError::NonZeroExit`; a missing verdict file
/// is reported as `Ok(None)` so callers can treat it as inconclusive without it bubbling
/// up as an error.
pub async fn run_review<E: Executor>(
    executor: &E,
    request: &ExecutionRequest,
    verdict_path: &Path,
    log_sink: Option<&Path>,
) -> Result<Option<ReviewVerdict>, ExecutorError> {
    let result = executor.execute(request, log_sink).await?;
    if result.exit_code != 0 {
        return Err(ExecutorError::non_zero_exit(
            ReviewerExecutor::PROGRAM,
            result.exit_code,
        ));
    }
    read_review(verdict_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockExecutor;
    use crate::traits::{McpServer, SandboxConfig, SandboxPolicy};
    use std::path::PathBuf;

    fn make_request(mode: ExecutionMode) -> ExecutionRequest {
        ExecutionRequest {
            context: "Review this diff:\n--- diff goes here ---".to_string(),
            mode,
            working_dir: PathBuf::from("/tmp/project"),
            sandbox: SandboxConfig {
                enabled: SandboxPolicy::Never,
                extra_allow: vec![],
                block_network: false,
            },
            mcp_servers: vec![],
        }
    }

    fn make_executor() -> ReviewerExecutor {
        ReviewerExecutor {
            system_prompt: "You are a code-review agent.".to_string(),
        }
    }

    // -- build_args -----------------------------------------------------

    #[test]
    fn build_args_interactive_with_system_prompt() {
        let exec = make_executor();
        let request = make_request(ExecutionMode::Interactive);
        let args = exec.build_args(&request);
        assert_eq!(
            args,
            vec!["--system-prompt", "You are a code-review agent."]
        );
    }

    #[test]
    fn build_args_accept_with_system_prompt() {
        let exec = make_executor();
        let request = make_request(ExecutionMode::Accept);
        let args = exec.build_args(&request);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "acceptEdits",
                "--system-prompt",
                "You are a code-review agent."
            ]
        );
    }

    #[test]
    fn build_args_yolo_passes_diff_context() {
        let exec = make_executor();
        let request = make_request(ExecutionMode::Yolo);
        let args = exec.build_args(&request);
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&request.context));
        assert!(args.contains(&"--system-prompt".to_string()));
    }

    #[test]
    fn build_args_empty_system_prompt_omitted() {
        let exec = ReviewerExecutor {
            system_prompt: String::new(),
        };
        let request = make_request(ExecutionMode::Interactive);
        let args = exec.build_args(&request);
        assert!(args.is_empty());
    }

    #[test]
    fn build_args_with_mcp_servers() {
        let exec = make_executor();
        let mut request = make_request(ExecutionMode::Interactive);
        request.mcp_servers = vec![McpServer {
            name: "fs".to_string(),
            command: "fs-server".to_string(),
        }];
        let args = exec.build_args(&request);
        assert_eq!(
            args,
            vec![
                "--system-prompt",
                "You are a code-review agent.",
                "--mcp-server",
                "fs -- fs-server",
            ]
        );
    }

    // -- read_review (parsing) ------------------------------------------

    #[test]
    fn read_review_parses_valid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("review.json");
        std::fs::write(
            &path,
            r#"{"summary":"adds new feature","confidence":0.92,"risks":["touches auth path"]}"#,
        )
        .expect("write");

        let v = read_review(&path).expect("read").expect("some");
        assert_eq!(v.summary, "adds new feature");
        assert!((v.confidence - 0.92).abs() < f32::EPSILON);
        assert_eq!(v.risks, vec!["touches auth path".to_string()]);
    }

    #[test]
    fn read_review_defaults_empty_risks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("review.json");
        std::fs::write(&path, r#"{"summary":"docs only","confidence":0.99}"#).expect("write");

        let v = read_review(&path).expect("read").expect("some");
        assert!(v.risks.is_empty());
    }

    #[test]
    fn read_review_missing_file_is_inconclusive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.json");
        let v = read_review(&path).expect("read");
        assert!(v.is_none());
    }

    #[test]
    fn read_review_malformed_returns_err() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.json");
        std::fs::write(&path, r#"{"summary":"missing confidence"}"#).expect("write");

        let err = read_review(&path).unwrap_err();
        assert!(matches!(err, ExecutorError::ReviewParse { .. }));
    }

    #[test]
    fn read_review_invalid_json_returns_err() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("garbage.json");
        std::fs::write(&path, b"not json at all").expect("write");

        let err = read_review(&path).unwrap_err();
        assert!(matches!(err, ExecutorError::ReviewParse { .. }));
    }

    #[test]
    fn read_review_wrong_type_returns_err() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("typed.json");
        // confidence as a string, not a number
        std::fs::write(&path, r#"{"summary":"x","confidence":"high","risks":[]}"#).expect("write");

        let err = read_review(&path).unwrap_err();
        assert!(matches!(err, ExecutorError::ReviewParse { .. }));
    }

    // -- confidence clamping --------------------------------------------

    #[test]
    fn confidence_clamped_above_one() {
        let v = ReviewVerdict {
            summary: "x".into(),
            confidence: 1.2,
            risks: vec![],
        };
        assert!((v.confidence_clamped() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn confidence_clamped_below_zero() {
        let v = ReviewVerdict {
            summary: "x".into(),
            confidence: -0.5,
            risks: vec![],
        };
        assert!((v.confidence_clamped() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn confidence_clamped_in_range_unchanged() {
        let v = ReviewVerdict {
            summary: "x".into(),
            confidence: 0.42,
            risks: vec![],
        };
        assert!((v.confidence_clamped() - 0.42).abs() < f32::EPSILON);
    }

    // -- run_review (full flow with mock executor) ----------------------

    #[tokio::test]
    async fn run_review_full_flow_success() {
        let dir = tempfile::tempdir().expect("tempdir");
        let verdict_path = dir.path().join("review_verdict.json");
        // Simulate the agent writing the verdict file before exit.
        std::fs::write(
            &verdict_path,
            r#"{"summary":"refactors db pool","confidence":0.78,"risks":["concurrent migrations"]}"#,
        )
        .expect("write");

        let exec = MockExecutor::success();
        let request = make_request(ExecutionMode::Yolo);

        let verdict = run_review(&exec, &request, &verdict_path, None)
            .await
            .expect("run_review")
            .expect("verdict present");

        assert_eq!(verdict.summary, "refactors db pool");
        assert!((verdict.confidence - 0.78).abs() < f32::EPSILON);
        assert_eq!(verdict.risks, vec!["concurrent migrations".to_string()]);
    }

    #[tokio::test]
    async fn run_review_executor_failure_propagates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let verdict_path = dir.path().join("review_verdict.json");

        let exec = MockExecutor::failure(2);
        let request = make_request(ExecutionMode::Yolo);

        let err = run_review(&exec, &request, &verdict_path, None)
            .await
            .unwrap_err();
        assert!(matches!(err, ExecutorError::NonZeroExit { code: 2, .. }));
    }

    #[tokio::test]
    async fn run_review_missing_verdict_is_inconclusive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let verdict_path = dir.path().join("never-written.json");

        let exec = MockExecutor::success();
        let request = make_request(ExecutionMode::Yolo);

        let v = run_review(&exec, &request, &verdict_path, None)
            .await
            .expect("run_review ok");
        assert!(v.is_none());
    }
}
