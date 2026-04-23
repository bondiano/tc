use std::io::Write;
use std::process::Command;

use tc_core::error::CoreError;
use tc_core::task::TaskId;

use crate::error::CliError;
use crate::output;

pub fn run(id: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;

    let task_id = TaskId(id.to_string());
    let idx = tasks
        .iter()
        .position(|t| t.id == task_id)
        .ok_or_else(|| CoreError::TaskNotFound(id.to_string()))?;

    let yaml = serde_yaml_ng::to_string(&tasks[idx])
        .map_err(|e| CliError::user(format!("Failed to serialize task: {e}")))?;

    let mut tmpfile = tempfile::NamedTempFile::with_suffix(".yaml")
        .map_err(|e| CliError::user(format!("Failed to create temp file: {e}")))?;

    tmpfile
        .write_all(yaml.as_bytes())
        .map_err(|e| CliError::user(format!("Failed to write temp file: {e}")))?;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor)
        .arg(tmpfile.path())
        .status()
        .map_err(|e| CliError::user(format!("Failed to launch editor '{editor}': {e}")))?;

    if !status.success() {
        return Err(CliError::user(format!(
            "Editor exited with status: {status}"
        )));
    }

    let edited = std::fs::read_to_string(tmpfile.path())
        .map_err(|e| CliError::user(format!("Failed to read edited file: {e}")))?;

    if edited == yaml {
        output::print_warning("No changes made");
        return Ok(());
    }

    let edited_task: tc_core::task::Task = serde_yaml_ng::from_str(&edited)
        .map_err(|e| CliError::user(format!("Invalid YAML: {e}")))?;

    if edited_task.id != task_id {
        return Err(CliError::user(format!(
            "Task ID cannot be changed (was {id}, got {})",
            edited_task.id
        )));
    }

    tasks[idx] = edited_task;
    store.save_tasks(&tasks)?;

    output::print_success(&format!("Updated {id}"));
    Ok(())
}
