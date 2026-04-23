//! Mock executor for testing. Available via the `mock` feature or in test builds.

use std::path::Path;
use std::time::Duration;

use crate::error::ExecutorError;
use crate::traits::{ExecutionRequest, ExecutionResult, Executor};

/// A configurable mock executor for testing spawn/scheduler logic.
///
/// Configure the desired exit code and optional delay before completion.
pub struct MockExecutor {
    pub exit_code: i32,
    pub delay: Option<Duration>,
}

impl MockExecutor {
    pub fn success() -> Self {
        Self {
            exit_code: 0,
            delay: None,
        }
    }

    pub fn failure(code: i32) -> Self {
        Self {
            exit_code: code,
            delay: None,
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

impl Executor for MockExecutor {
    fn build_command(
        &self,
        _request: &ExecutionRequest,
    ) -> Result<tokio::process::Command, ExecutorError> {
        // If there's a delay, use sleep + exit
        if let Some(delay) = self.delay {
            let secs = delay.as_secs_f64();
            let mut sleep_cmd = tokio::process::Command::new("sh");
            sleep_cmd.args(["-c", &format!("sleep {} && exit {}", secs, self.exit_code)]);
            return Ok(sleep_cmd);
        }

        // Build a command that exits with the configured code
        if self.exit_code == 0 {
            Ok(tokio::process::Command::new("true"))
        } else {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args(["-c", &format!("exit {}", self.exit_code)]);
            Ok(cmd)
        }
    }

    async fn execute(
        &self,
        _request: &ExecutionRequest,
        _log_sink: Option<&Path>,
    ) -> Result<ExecutionResult, ExecutorError> {
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }

        Ok(ExecutionResult {
            exit_code: self.exit_code,
            log_path: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{ExecutionMode, SandboxConfig, SandboxPolicy};
    use std::path::PathBuf;

    fn test_request() -> ExecutionRequest {
        ExecutionRequest {
            context: "test context".into(),
            mode: ExecutionMode::Yolo,
            working_dir: PathBuf::from("/tmp"),
            sandbox: SandboxConfig {
                enabled: SandboxPolicy::Never,
                extra_allow: vec![],
                block_network: false,
            },
            mcp_servers: vec![],
        }
    }

    #[tokio::test]
    async fn mock_success() {
        let exec = MockExecutor::success();
        let result = exec.execute(&test_request(), None).await.unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn mock_failure() {
        let exec = MockExecutor::failure(1);
        let result = exec.execute(&test_request(), None).await.unwrap();
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn mock_build_command_success() {
        let exec = MockExecutor::success();
        let cmd = exec.build_command(&test_request()).unwrap();
        let program = cmd.as_std().get_program();
        assert_eq!(program, "true");
    }

    #[test]
    fn mock_build_command_failure() {
        let exec = MockExecutor::failure(42);
        let cmd = exec.build_command(&test_request()).unwrap();
        let program = cmd.as_std().get_program();
        assert_eq!(program, "sh");
    }
}
