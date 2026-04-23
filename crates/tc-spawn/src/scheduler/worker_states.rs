use std::path::Path;

use crate::error::SpawnError;
use crate::process::WorkerState;

/// List all worker state files from the workers directory.
///
/// Silently skips unreadable/corrupt JSON files -- callers treat the
/// returned list as a best-effort view of what's running.
pub fn list_worker_states(workers_dir: &Path) -> Result<Vec<WorkerState>, SpawnError> {
    let entries = match std::fs::read_dir(workers_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(SpawnError::io("read workers dir", e)),
    };

    let mut states = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| SpawnError::io("read workers entry", e))?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json")
            && let Ok(state) = WorkerState::load(&path)
        {
            states.push(state);
        }
    }

    Ok(states)
}
