use std::path::Path;

use tokio::process::{Child, Command};

use crate::error::ExecutorError;
use crate::traits::ExecutionResult;

/// Pipe a child's stdout and stderr into a single log file.
///
/// Uses `tokio::io::copy` so the runtime is never blocked. Both streams
/// are copied into separate `tokio::fs::File` handles against the same
/// underlying file -- kernel append semantics keep writes coherent.
pub fn pipe_child_to_log(child: &mut Child, sink: &Path) -> Result<(), ExecutorError> {
    if let Some(parent) = sink.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ExecutorError::log_write(sink, e))?;
    }
    let std_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(sink)
        .map_err(|e| ExecutorError::log_write(sink, e))?;

    if let Some(mut stdout) = child.stdout.take() {
        let clone = std_file
            .try_clone()
            .map_err(|e| ExecutorError::log_write(sink, e))?;
        let mut out = tokio::fs::File::from_std(clone);
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut stdout, &mut out).await;
        });
    }

    if let Some(mut stderr) = child.stderr.take() {
        let clone = std_file
            .try_clone()
            .map_err(|e| ExecutorError::log_write(sink, e))?;
        let mut err = tokio::fs::File::from_std(clone);
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut stderr, &mut err).await;
        });
    }

    Ok(())
}

/// Spawn a built command, optionally pipe its output to a log file, and
/// wait for completion. Centralizes the pipe-spawn-wait dance that
/// ClaudeExecutor/OpencodeExecutor/TesterExecutor previously duplicated.
///
/// `program_label` is the name used in spawn-failure errors (typically the
/// executor's binary name like "claude" or "opencode").
pub async fn spawn_and_wait(
    mut cmd: Command,
    log_sink: Option<&Path>,
    program_label: &str,
) -> Result<ExecutionResult, ExecutorError> {
    if log_sink.is_some() {
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| ExecutorError::spawn_failed(program_label.to_string(), e))?;

    let log_path = if let Some(sink) = log_sink {
        pipe_child_to_log(&mut child, sink)?;
        Some(sink.to_path_buf())
    } else {
        None
    };

    let status = child
        .wait()
        .await
        .map_err(|e| ExecutorError::spawn_failed(program_label.to_string(), e))?;

    Ok(ExecutionResult {
        exit_code: status.code().unwrap_or(-1),
        log_path,
    })
}
