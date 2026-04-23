use std::path::Path;

use tokio::process::Child;

use crate::error::ExecutorError;

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
