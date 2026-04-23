use tc_core::dag::TaskDag;

use crate::error::CliError;
use crate::output;

pub fn run() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;

    if tasks.is_empty() {
        println!("No tasks to validate.");
        return Ok(());
    }

    match TaskDag::from_tasks(&tasks) {
        Ok(dag) => {
            let topo = dag.topological_order()?;
            let edge_count: usize = tasks.iter().map(|t| t.depends_on.len()).sum();
            output::print_success(&format!(
                "DAG valid: {} tasks, {} edges, 0 cycles",
                topo.len(),
                edge_count
            ));
            Ok(())
        }
        Err(e) => {
            output::print_error(&format!("{e}"));
            Err(e.into())
        }
    }
}
