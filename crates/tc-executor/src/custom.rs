use std::path::Path;

use tc_core::config::CustomBackendConfig;
use tempfile::NamedTempFile;

use crate::error::ExecutorError;
use crate::io::pipe_child_to_log;
use crate::sandbox::{detect_provider, wrap_with_sandbox};
use crate::traits::{ExecutionMode, ExecutionRequest, ExecutionResult, Executor};

const CTX_PLACEHOLDER: &str = "{context}";
const CTX_FILE_PLACEHOLDER: &str = "{context_file}";
const CTX_FILE_DRYRUN: &str = "<context_file>";

/// Generic executor driven by `CustomBackendConfig` from `.tc/config.yaml`.
///
/// Used for backends whose CLI flags we don't want to hardcode (codex, pi.dev).
/// The command binary and per-mode arg templates come from the config so users
/// can adapt to upstream CLI changes without a tc release.
///
/// Templates support two placeholders:
///   - `{context}` -- substituted inline with the prompt string.
///   - `{context_file}` -- substituted with the path to a temp file holding the
///     prompt. Useful when the prompt exceeds argv length limits.
pub struct CustomExecutor {
    name: String,
    config: CustomBackendConfig,
}

impl CustomExecutor {
    pub fn new(name: impl Into<String>, config: CustomBackendConfig) -> Self {
        Self {
            name: name.into(),
            config,
        }
    }

    pub fn codex(config: CustomBackendConfig) -> Self {
        Self::new("codex", config)
    }

    pub fn pi(config: CustomBackendConfig) -> Self {
        Self::new("pi", config)
    }

    pub fn gemini(config: CustomBackendConfig) -> Self {
        Self::new("gemini", config)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn template_args(&self, mode: &ExecutionMode) -> &[String] {
        match mode {
            ExecutionMode::Interactive => &self.config.interactive_args,
            ExecutionMode::Accept => &self.config.accept_args,
            ExecutionMode::Yolo => &self.config.yolo_args,
        }
    }

    fn expand_args(&self, request: &ExecutionRequest, ctx_file: Option<&Path>) -> Vec<String> {
        let ctx = request.context.as_str();
        let ctx_file_str = match ctx_file {
            Some(p) => p.to_string_lossy().into_owned(),
            None => CTX_FILE_DRYRUN.to_string(),
        };
        self.template_args(&request.mode)
            .iter()
            .map(|tpl| {
                tpl.replace(CTX_PLACEHOLDER, ctx)
                    .replace(CTX_FILE_PLACEHOLDER, &ctx_file_str)
            })
            .collect()
    }

    fn wrap_if_yolo(
        &self,
        base_args: Vec<String>,
        request: &ExecutionRequest,
    ) -> (String, Vec<String>) {
        if !matches!(request.mode, ExecutionMode::Yolo) {
            return (self.config.command.clone(), base_args);
        }
        let provider = detect_provider(&request.sandbox);
        wrap_with_sandbox(
            &self.config.command,
            &base_args,
            &provider,
            &request.sandbox,
            &request.working_dir,
        )
    }
}

impl Executor for CustomExecutor {
    fn build_command(
        &self,
        request: &ExecutionRequest,
    ) -> Result<tokio::process::Command, ExecutorError> {
        if which::which(&self.config.command).is_err() {
            return Err(ExecutorError::not_found(&self.config.command));
        }

        let base_args = self.expand_args(request, None);
        let (program, args) = self.wrap_if_yolo(base_args, request);

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
        if which::which(&self.config.command).is_err() {
            return Err(ExecutorError::not_found(&self.config.command));
        }

        // Write context to a tempfile so template args can reference it via
        // {context_file}. Held for the duration of the child process so the
        // file is not prematurely unlinked.
        let uses_ctx_file = self
            .template_args(&request.mode)
            .iter()
            .any(|a| a.contains(CTX_FILE_PLACEHOLDER));

        let ctx_file = if uses_ctx_file {
            let tmp = NamedTempFile::new()
                .map_err(|e| ExecutorError::spawn_failed(format!("{}:ctx", self.name), e))?;
            std::fs::write(tmp.path(), &request.context)
                .map_err(|e| ExecutorError::spawn_failed(format!("{}:ctx-write", self.name), e))?;
            Some(tmp)
        } else {
            None
        };

        let ctx_path = ctx_file.as_ref().map(|f| f.path());
        let base_args = self.expand_args(request, ctx_path);
        let (program, args) = self.wrap_if_yolo(base_args, request);

        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args);
        cmd.current_dir(&request.working_dir);

        if log_sink.is_some() {
            cmd.stdout(std::process::Stdio::piped());
            cmd.stderr(std::process::Stdio::piped());
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| ExecutorError::spawn_failed(&self.config.command, e))?;

        let log_path = if let Some(sink) = log_sink {
            pipe_child_to_log(&mut child, sink)?;
            Some(sink.to_path_buf())
        } else {
            None
        };

        let status = child
            .wait()
            .await
            .map_err(|e| ExecutorError::spawn_failed(&self.config.command, e))?;

        // Keep ctx_file alive until wait() returns.
        drop(ctx_file);

        Ok(ExecutionResult {
            exit_code: status.code().unwrap_or(-1),
            log_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{SandboxConfig, SandboxPolicy};
    use std::path::PathBuf;

    fn codex_cfg() -> CustomBackendConfig {
        CustomBackendConfig {
            command: "codex".into(),
            yolo_args: vec![
                "exec".into(),
                "--dangerously-bypass-approvals-and-sandbox".into(),
                "{context}".into(),
            ],
            accept_args: vec!["--prompt".into(), "{context}".into()],
            interactive_args: vec![],
        }
    }

    fn pi_cfg() -> CustomBackendConfig {
        CustomBackendConfig {
            command: "pi".into(),
            yolo_args: vec![
                "run".into(),
                "--yolo".into(),
                "--prompt-file".into(),
                "{context_file}".into(),
            ],
            accept_args: vec![],
            interactive_args: vec![],
        }
    }

    fn request(mode: ExecutionMode, ctx: &str) -> ExecutionRequest {
        ExecutionRequest {
            context: ctx.into(),
            mode,
            working_dir: PathBuf::from("/tmp"),
            sandbox: SandboxConfig {
                enabled: SandboxPolicy::Never,
                extra_allow: vec![],
                block_network: false,
            },
            mcp_servers: vec![],
        }
    }

    #[test]
    fn codex_yolo_substitutes_context() {
        let exec = CustomExecutor::codex(codex_cfg());
        let req = request(ExecutionMode::Yolo, "resolve conflict");
        let args = exec.expand_args(&req, None);
        assert_eq!(
            args,
            vec![
                "exec",
                "--dangerously-bypass-approvals-and-sandbox",
                "resolve conflict"
            ]
        );
    }

    #[test]
    fn codex_accept_uses_accept_template() {
        let exec = CustomExecutor::codex(codex_cfg());
        let req = request(ExecutionMode::Accept, "hi");
        let args = exec.expand_args(&req, None);
        assert_eq!(args, vec!["--prompt", "hi"]);
    }

    #[test]
    fn pi_substitutes_context_file_path() {
        let exec = CustomExecutor::pi(pi_cfg());
        let req = request(ExecutionMode::Yolo, "long prompt");
        let path = PathBuf::from("/tmp/ctx.txt");
        let args = exec.expand_args(&req, Some(&path));
        assert_eq!(args, vec!["run", "--yolo", "--prompt-file", "/tmp/ctx.txt"]);
    }

    #[test]
    fn pi_leaves_placeholder_without_file() {
        let exec = CustomExecutor::pi(pi_cfg());
        let req = request(ExecutionMode::Yolo, "x");
        let args = exec.expand_args(&req, None);
        assert!(args.iter().any(|a| a == CTX_FILE_DRYRUN));
    }

    #[test]
    fn empty_interactive_template_yields_no_args() {
        let exec = CustomExecutor::codex(codex_cfg());
        let req = request(ExecutionMode::Interactive, "ignored");
        assert!(exec.expand_args(&req, None).is_empty());
    }

    #[tokio::test]
    async fn missing_binary_reports_not_found() {
        let cfg = CustomBackendConfig {
            command: "this-binary-does-not-exist-xyz".into(),
            yolo_args: vec!["{context}".into()],
            accept_args: vec![],
            interactive_args: vec![],
        };
        let exec = CustomExecutor::new("phantom", cfg);
        let req = request(ExecutionMode::Yolo, "irrelevant");
        let err = exec.execute(&req, None).await.unwrap_err();
        assert!(matches!(err, ExecutorError::NotFound { .. }));
    }
}
