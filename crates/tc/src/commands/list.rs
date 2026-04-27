use std::collections::BTreeMap;

use chrono::Local;
use tc_core::dag::TaskDag;
use tc_core::filter::Filter;
use tc_core::status::{StatusId, StatusMachine};
use tc_core::task::Task;

use crate::cli::ListArgs;
use crate::error::CliError;
use crate::output;

pub fn run(args: ListArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;

    if tasks.is_empty() {
        println!("No tasks. Run `tc add` to create one.");
        return Ok(());
    }

    let sm = StatusMachine::new(config.statuses);
    let dag = TaskDag::from_tasks(&tasks)?;

    // Concatenate positional args back into a single query string. clap splits
    // them on whitespace if the user runs `tc list priority:p1 tag:foo`
    // without quotes, so re-joining preserves the parser's contract.
    let query = args.query.join(" ");
    let filter =
        Filter::parse(&query).map_err(|e| CliError::user(format!("invalid filter: {e}")))?;

    let today = Local::now().date_naive();
    let filtered = filter_tasks(&tasks, &args, &dag, &sm, &filter, today);

    if args.ids_only {
        for t in &filtered {
            println!("{}", t.id.0);
        }
        return Ok(());
    }

    if filtered.is_empty() {
        println!("No matching tasks.");
        return Ok(());
    }

    let grouped = group_by_epic(&filtered);
    for (epic, epic_tasks) in &grouped {
        println!("\n[{epic}]");
        println!("{}", output::format_task_refs(epic_tasks));
    }

    Ok(())
}

fn filter_tasks<'a>(
    tasks: &'a [Task],
    args: &ListArgs,
    dag: &TaskDag,
    sm: &StatusMachine,
    filter: &Filter,
    today: chrono::NaiveDate,
) -> Vec<&'a Task> {
    let ready_ids = if args.ready {
        Some(dag.compute_ready(tasks, sm))
    } else {
        None
    };

    tasks
        .iter()
        .filter(|t| {
            if let Some(ref ids) = ready_ids {
                return ids.contains(&t.id);
            }
            if args.blocked {
                return t.status == StatusId("blocked".to_string());
            }
            true
        })
        .filter(|t| {
            args.epic
                .as_ref()
                .is_none_or(|e| t.epic.eq_ignore_ascii_case(e))
        })
        .filter(|t| filter.matches(t, today))
        .collect()
}

fn group_by_epic<'a>(tasks: &[&'a Task]) -> BTreeMap<String, Vec<&'a Task>> {
    let mut map: BTreeMap<String, Vec<&'a Task>> = BTreeMap::new();
    for task in tasks {
        map.entry(task.epic.clone()).or_default().push(task);
    }
    map
}
