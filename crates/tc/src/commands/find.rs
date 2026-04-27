use tc_core::fuzzy;

use crate::cli::FindArgs;
use crate::error::CliError;
use crate::output;

pub fn run(args: FindArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;

    if tasks.is_empty() {
        println!("No tasks. Run `tc add` to create one.");
        return Ok(());
    }

    let hits = fuzzy::search(&tasks, &args.query, args.limit);

    if hits.is_empty() {
        println!("No matches for {:?}", args.query);
        return Ok(());
    }

    if args.ids_only {
        for h in &hits {
            println!("{}", h.id);
        }
        return Ok(());
    }

    let matched: Vec<&_> = hits
        .iter()
        .filter_map(|h| tasks.iter().find(|t| t.id == h.id))
        .collect();

    println!("{}", output::format_task_refs(&matched));
    Ok(())
}
