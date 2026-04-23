use std::path::{Path, PathBuf};

use crate::error::ExecutorError;

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub context: String,
    pub mode: ExecutionMode,
    pub working_dir: PathBuf,
    pub sandbox: SandboxConfig,
    pub mcp_servers: Vec<McpServer>,
}

#[derive(Debug, Clone)]
pub enum ExecutionMode {
    Interactive,
    Accept,
    Yolo,
}

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub enabled: SandboxPolicy,
    pub extra_allow: Vec<PathBuf>,
    pub block_network: bool,
}

#[derive(Debug, Clone)]
pub enum SandboxPolicy {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone)]
pub struct McpServer {
    pub name: String,
    pub command: String,
}

#[derive(Debug)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub log_path: Option<PathBuf>,
}

pub trait Executor: Send + Sync {
    /// Build the command to execute (without spawning).
    fn build_command(
        &self,
        request: &ExecutionRequest,
    ) -> Result<tokio::process::Command, ExecutorError>;

    /// Spawn and wait for completion, piping output to log.
    fn execute(
        &self,
        request: &ExecutionRequest,
        log_sink: Option<&Path>,
    ) -> impl std::future::Future<Output = Result<ExecutionResult, ExecutorError>> + Send;
}
