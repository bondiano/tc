use tc_core::dag::TaskDag;
use tc_core::status::StatusMachine;

use crate::error::CliError;
use crate::output;

pub fn run() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;

    if tasks.is_empty() {
        println!("No tasks. Run `tc add` to create one.");
        return Ok(());
    }

    let sm = StatusMachine::new(config.statuses);
    let dag = TaskDag::from_tasks(&tasks)?;

    let topo = dag.topological_order()?;
    let ready = dag.compute_ready(&tasks, &sm);

    let next = topo.iter().find(|id| ready.contains(id));

    match next.and_then(|id| tasks.iter().find(|t| t.id == *id)) {
        Some(task) => {
            println!("{}", output::format_detail(task));
        }
        None => {
            output::print_success("All done!");
        }
    }

    Ok(())
}
