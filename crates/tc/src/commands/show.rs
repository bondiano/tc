use tc_core::dag::TaskDag;
use tc_core::error::CoreError;

use crate::error::CliError;
use crate::output;

pub fn run(id: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;

    let task = tasks
        .iter()
        .find(|t| t.id.0 == id)
        .ok_or_else(|| CoreError::TaskNotFound(id.to_string()))?;

    println!("{}", output::format_detail(task));

    if let Ok(dag) = TaskDag::from_tasks(&tasks) {
        let deps = dag.dependencies(&task.id);
        let dependents = dag.dependents(&task.id);

        if !deps.is_empty() {
            let dep_strs: Vec<String> = deps.iter().map(|d| d.0.clone()).collect();
            println!("\nDeps:       {} --> {id}", dep_strs.join(", "));
        }
        if !dependents.is_empty() {
            let dep_strs: Vec<String> = dependents.iter().map(|d| d.0.clone()).collect();
            println!("Dependents: {id} --> {}", dep_strs.join(", "));
        }
    }

    Ok(())
}
