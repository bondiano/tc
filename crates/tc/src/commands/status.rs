use tc_core::dag::TaskDag;
use tc_core::error::CoreError;
use tc_core::status::{StatusId, StatusMachine};

use crate::error::CliError;
use crate::output;

fn find_task_index(tasks: &[tc_core::task::Task], id: &str) -> Result<usize, CliError> {
    tasks
        .iter()
        .position(|t| t.id.0 == id)
        .ok_or_else(|| CoreError::TaskNotFound(id.to_string()).into())
}

fn ensure_not_terminal(task: &tc_core::task::Task, sm: &StatusMachine) -> Result<(), CliError> {
    if sm.is_terminal(&task.status) {
        return Err(CoreError::AlreadyTerminal {
            task: task.id.0.clone(),
            status: task.status.0.clone(),
        }
        .into());
    }
    Ok(())
}

pub fn run_done(id: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;
    let config = store.load_config()?;
    let sm = StatusMachine::new(config.statuses);

    let idx = find_task_index(&tasks, id)?;
    ensure_not_terminal(&tasks[idx], &sm)?;

    let dag = TaskDag::from_tasks(&tasks)?;
    let task_id = tasks[idx].id.clone();
    let deps = dag.dependencies(&task_id);
    let unresolved: Vec<String> = deps
        .iter()
        .filter(|dep_id| {
            tasks
                .iter()
                .find(|t| t.id == **dep_id)
                .is_some_and(|t| !sm.is_terminal(&t.status))
        })
        .map(|d| d.0.clone())
        .collect();

    if !unresolved.is_empty() {
        return Err(CoreError::unresolved_deps(id, &unresolved).into());
    }

    tasks[idx].status = StatusId("done".to_string());
    store.save_tasks(&tasks)?;

    output::print_success(&format!("Marked {id} as done"));

    let dag = TaskDag::from_tasks(&tasks)?;
    let unblocked = dag.unblocked_by(&task_id, &tasks, &sm);
    if !unblocked.is_empty() {
        let names: Vec<String> = unblocked.iter().map(|u| u.0.clone()).collect();
        output::print_success(&format!("Unblocked: {}", names.join(", ")));
    }

    Ok(())
}

pub fn run_block(id: &str, reason: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;
    let config = store.load_config()?;
    let sm = StatusMachine::new(config.statuses);

    let idx = find_task_index(&tasks, id)?;
    ensure_not_terminal(&tasks[idx], &sm)?;

    tasks[idx].status = StatusId("blocked".to_string());
    if !tasks[idx].notes.is_empty() {
        tasks[idx].notes.push('\n');
    }
    tasks[idx].notes.push_str(&format!("BLOCKED: {reason}"));

    store.save_tasks(&tasks)?;
    output::print_warning(&format!("{id} blocked: {reason}"));

    Ok(())
}

pub fn run_set(id: &str, status: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;
    let config = store.load_config()?;
    let sm = StatusMachine::new(config.statuses);

    let new_status = StatusId(status.to_string());
    sm.validate(&new_status)?;

    let idx = find_task_index(&tasks, id)?;
    ensure_not_terminal(&tasks[idx], &sm)?;

    tasks[idx].status = new_status;

    store.save_tasks(&tasks)?;
    output::print_success(&format!("{id} -> {status}"));

    Ok(())
}
