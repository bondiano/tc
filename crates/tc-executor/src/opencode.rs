use std::path::Path;

use crate::error::ExecutorError;
use crate::io::pipe_child_to_log;
use crate::sandbox::{detect_provider, wrap_with_sandbox};
use crate::traits::{ExecutionMode, ExecutionRequest, ExecutionResult, Executor};

pub struct OpencodeExecutor;

impl OpencodeExecutor {
    const PROGRAM: &'static str = "opencode";

    fn build_args(request: &ExecutionRequest) -> Vec<String> {
        let mut args = Vec::new();

        match request.mode {
            ExecutionMode::Interactive => {}
            ExecutionMode::Accept => {
                args.push("--yes".to_string());
            }
            ExecutionMode::Yolo => {
                args.push("--yes".to_string());
                args.push("--dangerously-skip-permissions".to_string());
            }
        }

        if !request.context.is_empty() && matches!(request.mode, ExecutionMode::Yolo) {
            args.push("--prompt".to_string());
            args.push(request.context.clone());
        }

        args
    }
}

impl Executor for OpencodeExecutor {
    fn build_command(
        &self,
        request: &ExecutionRequest,
    ) -> Result<tokio::process::Command, ExecutorError> {
        if which::which(Self::PROGRAM).is_err() {
            return Err(ExecutorError::not_found(Self::PROGRAM));
        }

        let base_args = Self::build_args(request);

        let (program, args) = if matches!(request.mode, ExecutionMode::Yolo) {
            let provider = detect_provider(&request.sandbox);
            wrap_with_sandbox(
                Self::PROGRAM,
                &base_args,
                &provider,
                &request.sandbox,
                &request.working_dir,
            )
        } else {
            (Self::PROGRAM.to_string(), base_args)
        };

        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args);
        cmd.current_dir(&request.working_dir);

        Ok(cmd)
    }

    async fn execute(
        &self,
        request: &ExecutionRequest,
        log_sink: Option<&Path>,
    ) -> Result<ExecutionResult, ExecutorError> {
        let mut cmd = self.build_command(request)?;

        if log_sink.is_some() {
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| ExecutorError::spawn_failed(Self::PROGRAM, e))?;

        let log_path = if let Some(sink) = log_sink {
            pipe_child_to_log(&mut child, sink)?;
            Some(sink.to_path_buf())
        } else {
            None
        };

        let status = child
            .wait()
            .await
            .map_err(|e| ExecutorError::spawn_failed(Self::PROGRAM, e))?;

        let exit_code = status.code().unwrap_or(-1);

        Ok(ExecutionResult {
            exit_code,
            log_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{SandboxConfig, SandboxPolicy};
    use std::path::PathBuf;

    fn make_request(mode: ExecutionMode) -> ExecutionRequest {
        ExecutionRequest {
            context: "Implement the feature".to_string(),
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

    #[test]
    fn build_args_interactive() {
        let request = make_request(ExecutionMode::Interactive);
        let args = OpencodeExecutor::build_args(&request);
        assert!(args.is_empty());
    }

    #[test]
    fn build_args_accept() {
        let request = make_request(ExecutionMode::Accept);
        let args = OpencodeExecutor::build_args(&request);
        assert_eq!(args, vec!["--yes"]);
    }

    #[test]
    fn build_args_yolo() {
        let request = make_request(ExecutionMode::Yolo);
        let args = OpencodeExecutor::build_args(&request);
        assert_eq!(
            args,
            vec![
                "--yes",
                "--dangerously-skip-permissions",
                "--prompt",
                "Implement the feature"
            ]
        );
    }

    #[test]
    fn not_found_error_format() {
        let err = ExecutorError::not_found("opencode");
        assert!(err.to_string().contains("opencode"));
        assert!(err.to_string().contains("not found"));
    }
}
