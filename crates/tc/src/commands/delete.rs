use std::io::{self, BufRead, Write};

use tc_core::dag::TaskDag;
use tc_core::error::CoreError;
use tc_core::task::TaskId;

use crate::cli::DeleteArgs;
use crate::error::CliError;
use crate::output;

pub fn run(args: DeleteArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;

    let task_id = TaskId(args.id.clone());
    let task = tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| CoreError::TaskNotFound(args.id.clone()))?;

    let title = task.title.clone();

    if !args.force {
        let dag = TaskDag::from_tasks(&tasks)?;
        let dependents = dag.dependents(&task_id);
        if !dependents.is_empty() {
            let dep_list: Vec<String> = dependents.iter().map(|d| d.to_string()).collect();
            return Err(CliError::user(format!(
                "Cannot delete {}: depended on by {}. Use --force to override.",
                args.id,
                dep_list.join(", ")
            )));
        }
    }

    if !confirm(&format!("Delete {id}: '{title}'?", id = args.id))? {
        output::print_warning("Cancelled");
        return Ok(());
    }

    if args.force {
        'cleanup_deps: for task in &mut tasks {
            task.depends_on.retain(|dep| *dep != task_id);
            continue 'cleanup_deps;
        }
    }

    tasks.retain(|t| t.id != task_id);
    store.save_tasks(&tasks)?;

    output::print_success(&format!("Deleted {}: '{title}'", args.id));
    Ok(())
}

fn confirm(prompt: &str) -> Result<bool, CliError> {
    eprint!("{prompt} (y/n) ");
    io::stderr()
        .flush()
        .map_err(|e| CliError::user(format!("Failed to flush: {e}")))?;

    let stdin = io::stdin();
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|e| CliError::user(format!("Failed to read input: {e}")))?;

    Ok(line.trim().eq_ignore_ascii_case("y"))
}
