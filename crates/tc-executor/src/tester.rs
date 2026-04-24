use std::path::Path;

use serde::Deserialize;

use crate::error::ExecutorError;
use crate::io::spawn_and_wait;
use crate::traits::{ExecutionMode, ExecutionRequest, ExecutionResult, Executor};

/// Verdict written by the tester agent to `.tc/.tester_verdict.json` before exit.
///
/// The agent is instructed (via system prompt) to write exactly one of:
/// - `{"verdict":"pass"}` when all acceptance criteria pass.
/// - `{"verdict":"fail","reason":"..."}` otherwise.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "verdict", rename_all = "lowercase")]
pub enum TesterVerdict {
    Pass {
        #[serde(default)]
        reason: Option<String>,
    },
    Fail {
        reason: String,
    },
}

/// Read a verdict file.
///
/// Returns `Ok(None)` if the file does not exist (inconclusive -- the tester
/// did not produce a verdict). Returns `Ok(Some(_))` on successful parse and
/// `Err` on I/O failure or malformed JSON.
pub fn read_verdict(path: &Path) -> Result<Option<TesterVerdict>, ExecutorError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ExecutorError::verdict_read(path, e)),
    };

    let verdict = serde_json::from_slice::<TesterVerdict>(&bytes)
        .map_err(|e| ExecutorError::verdict_parse(path, e))?;
    Ok(Some(verdict))
}

/// Executor that runs Claude as a testing/verification agent.
///
/// Adds a `--system-prompt` flag to configure Claude with tester-specific
/// instructions, on top of the standard mode and MCP server flags.
pub struct TesterExecutor {
    pub system_prompt: String,
}

impl TesterExecutor {
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

        // Tester system prompt
        if !self.system_prompt.is_empty() {
            args.push("--system-prompt".to_string());
            args.push(self.system_prompt.clone());
        }

        // MCP servers
        for mcp in &request.mcp_servers {
            args.push("--mcp-server".to_string());
            args.push(format!("{} -- {}", mcp.name, mcp.command));
        }

        args
    }
}

impl Executor for TesterExecutor {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{McpServer, SandboxConfig, SandboxPolicy};
    use std::path::PathBuf;

    fn make_request(mode: ExecutionMode) -> ExecutionRequest {
        ExecutionRequest {
            context: "Test the implementation".to_string(),
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

    fn make_executor() -> TesterExecutor {
        TesterExecutor {
            system_prompt: "You are a testing agent.".to_string(),
        }
    }

    #[test]
    fn build_args_interactive_with_system_prompt() {
        let exec = make_executor();
        let request = make_request(ExecutionMode::Interactive);
        let args = exec.build_args(&request);
        assert_eq!(args, vec!["--system-prompt", "You are a testing agent."]);
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
                "You are a testing agent."
            ]
        );
    }

    #[test]
    fn build_args_yolo_with_system_prompt() {
        let exec = make_executor();
        let request = make_request(ExecutionMode::Yolo);
        let args = exec.build_args(&request);
        assert_eq!(
            args,
            vec![
                "--permission-mode",
                "bypassPermissions",
                "--print",
                "Test the implementation",
                "--system-prompt",
                "You are a testing agent.",
            ]
        );
    }

    #[test]
    fn build_args_empty_system_prompt_omitted() {
        let exec = TesterExecutor {
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
        request.mcp_servers = vec![
            McpServer {
                name: "browser".to_string(),
                command: "npx playwright-mcp".to_string(),
            },
            McpServer {
                name: "fs".to_string(),
                command: "fs-server".to_string(),
            },
        ];
        let args = exec.build_args(&request);
        assert_eq!(
            args,
            vec![
                "--system-prompt",
                "You are a testing agent.",
                "--mcp-server",
                "browser -- npx playwright-mcp",
                "--mcp-server",
                "fs -- fs-server",
            ]
        );
    }

    #[test]
    fn build_args_yolo_with_mcp() {
        let exec = make_executor();
        let mut request = make_request(ExecutionMode::Yolo);
        request.mcp_servers = vec![McpServer {
            name: "browser".to_string(),
            command: "npx playwright-mcp".to_string(),
        }];
        let args = exec.build_args(&request);
        assert!(args.contains(&"--permission-mode".to_string()));
        assert!(args.contains(&"bypassPermissions".to_string()));
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--system-prompt".to_string()));
        assert_eq!(args.iter().filter(|a| *a == "--mcp-server").count(), 1);
    }

    // ── read_verdict ────────────────────────────────────────────────

    #[test]
    fn read_verdict_pass() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("verdict.json");
        std::fs::write(&path, r#"{"verdict":"pass"}"#).expect("write");
        let v = read_verdict(&path).expect("read").expect("some");
        assert_eq!(v, TesterVerdict::Pass { reason: None });
    }

    #[test]
    fn read_verdict_pass_with_reason() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("verdict.json");
        std::fs::write(&path, r#"{"verdict":"pass","reason":"all green"}"#).expect("write");
        let v = read_verdict(&path).expect("read").expect("some");
        assert_eq!(
            v,
            TesterVerdict::Pass {
                reason: Some("all green".into())
            }
        );
    }

    #[test]
    fn read_verdict_fail_with_reason() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("verdict.json");
        std::fs::write(&path, r#"{"verdict":"fail","reason":"login flow broken"}"#).expect("write");
        let v = read_verdict(&path).expect("read").expect("some");
        assert_eq!(
            v,
            TesterVerdict::Fail {
                reason: "login flow broken".into()
            }
        );
    }

    #[test]
    fn read_verdict_missing_returns_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.json");
        let v = read_verdict(&path).expect("read");
        assert!(v.is_none());
    }

    #[test]
    fn read_verdict_malformed_returns_err() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.json");
        std::fs::write(&path, r#"{"verdict":"maybe"}"#).expect("write");
        let err = read_verdict(&path).unwrap_err();
        assert!(matches!(err, ExecutorError::VerdictParse { .. }));
    }
}
