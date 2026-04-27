use std::io::Write;
use std::process::Command;

use tc_core::error::CoreError;
use tc_core::status::StatusId;
use tc_core::task::{Priority, TaskId};

use crate::cli::EditArgs;
use crate::error::CliError;
use crate::output;

pub fn run(args: EditArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let task_id = TaskId(args.id.clone());

    if args.has_any_patch() {
        return apply_patch(&store, &task_id, args);
    }
    open_in_editor(&store, &task_id)
}

/// Apply CLI flags atomically: load + mutate + save under a single
/// `Store::update_tasks` lock so concurrent `tc edit` invocations don't
/// stomp on each other.
fn apply_patch(
    store: &tc_storage::Store,
    task_id: &TaskId,
    args: EditArgs,
) -> Result<(), CliError> {
    let id_str = task_id.0.clone();

    store.update_tasks(|tasks| {
        let task = tasks
            .iter_mut()
            .find(|t| t.id == *task_id)
            .ok_or_else(|| CoreError::TaskNotFound(id_str.clone()))?;

        if let Some(title) = args.title {
            task.title = title;
        }
        if let Some(status) = args.status {
            task.status = StatusId(status);
        }
        if let Some(epic) = args.epic {
            task.epic = epic;
        }
        if let Some(p) = args.priority {
            task.priority = Priority::from(p);
        }
        if let Some(tags) = args.tags {
            task.tags = tags;
        }
        for tag in args.add_tags {
            if !task.tags.contains(&tag) {
                task.tags.push(tag);
            }
        }
        for tag in args.rm_tags {
            task.tags.retain(|t| t != &tag);
        }
        if let Some(due) = args.due {
            due.apply(&mut task.due);
        }
        if let Some(scheduled) = args.scheduled {
            scheduled.apply(&mut task.scheduled);
        }
        if let Some(estimate) = args.estimate {
            estimate.apply(&mut task.estimate);
        }
        for ac in args.add_acceptance_criteria {
            task.acceptance_criteria.push(ac);
        }
        Ok(())
    })?;

    output::print_success(&format!("Updated {id_str}"));
    Ok(())
}

fn open_in_editor(store: &tc_storage::Store, task_id: &TaskId) -> Result<(), CliError> {
    let mut tasks = store.load_tasks()?;

    let idx = tasks
        .iter()
        .position(|t| t.id == *task_id)
        .ok_or_else(|| CoreError::TaskNotFound(task_id.0.clone()))?;

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

    if edited_task.id != *task_id {
        return Err(CliError::user(format!(
            "Task ID cannot be changed (was {}, got {})",
            task_id.0, edited_task.id
        )));
    }

    tasks[idx] = edited_task;
    store.save_tasks(&tasks)?;

    output::print_success(&format!("Updated {}", task_id.0));
    Ok(())
}
