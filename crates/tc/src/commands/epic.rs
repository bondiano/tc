use std::collections::BTreeMap;

use tc_core::dag::TaskDag;
use tc_core::status::StatusMachine;
use tc_core::task::Task;

use crate::cli::EpicArgs;
use crate::cli::EpicCommands;
use crate::error::CliError;
use crate::output;

pub fn run(args: EpicArgs) -> Result<(), CliError> {
    match args.command {
        EpicCommands::List => run_list(),
        EpicCommands::Show { name } => run_show(&name),
        EpicCommands::Rename { old, new } => run_rename(&old, &new),
    }
}

fn run_list() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;

    if tasks.is_empty() {
        println!("No epics. Run `tc add` to create a task.");
        return Ok(());
    }

    let sm = StatusMachine::new(config.statuses);
    let dag = TaskDag::from_tasks(&tasks)?;
    let ready_ids = dag.compute_ready(&tasks, &sm);

    let epics = collect_epic_stats(&tasks, &sm, &ready_ids);

    for (name, stats) in &epics {
        let pct = if stats.total > 0 {
            stats.done * 100 / stats.total
        } else {
            0
        };
        println!(
            "{:<20} {}/{} done ({}%)  {} ready  {}",
            name,
            stats.done,
            stats.total,
            pct,
            stats.ready,
            progress_bar(stats.done, stats.total, 20),
        );
    }

    Ok(())
}

fn run_show(name: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;

    let epic_tasks: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.epic.eq_ignore_ascii_case(name))
        .collect();

    if epic_tasks.is_empty() {
        return Err(CliError::user(format!("Epic '{name}' not found")));
    }

    let sm = StatusMachine::new(config.statuses);
    let dag = TaskDag::from_tasks(&tasks)?;
    let ready_ids = dag.compute_ready(&tasks, &sm);

    let done = epic_tasks
        .iter()
        .filter(|t| sm.is_terminal(&t.status))
        .count();
    let ready = epic_tasks
        .iter()
        .filter(|t| ready_ids.contains(&t.id))
        .count();
    let blocked = epic_tasks
        .iter()
        .filter(|t| t.status.0 == "blocked")
        .count();

    println!("[{name}]");
    println!(
        "{done}/{} done, {ready} ready, {blocked} blocked",
        epic_tasks.len()
    );
    println!("{}", progress_bar(done, epic_tasks.len(), 30));
    println!();
    println!("{}", output::format_task_refs(&epic_tasks));

    Ok(())
}

fn run_rename(old: &str, new: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;

    let count = tasks
        .iter()
        .filter(|t| t.epic.eq_ignore_ascii_case(old))
        .count();

    if count == 0 {
        return Err(CliError::user(format!("Epic '{old}' not found")));
    }

    for task in &mut tasks {
        if task.epic.eq_ignore_ascii_case(old) {
            task.epic = new.to_string();
        }
    }

    store.save_tasks(&tasks)?;
    output::print_success(&format!("Renamed epic '{old}' -> '{new}' ({count} tasks)"));

    Ok(())
}

struct EpicStats {
    total: usize,
    done: usize,
    ready: usize,
}

fn collect_epic_stats(
    tasks: &[Task],
    sm: &StatusMachine,
    ready_ids: &[tc_core::task::TaskId],
) -> BTreeMap<String, EpicStats> {
    let mut epics: BTreeMap<String, EpicStats> = BTreeMap::new();
    for task in tasks {
        let entry = epics.entry(task.epic.clone()).or_insert(EpicStats {
            total: 0,
            done: 0,
            ready: 0,
        });
        entry.total += 1;
        if sm.is_terminal(&task.status) {
            entry.done += 1;
        }
        if ready_ids.contains(&task.id) {
            entry.ready += 1;
        }
    }
    epics
}

fn progress_bar(done: usize, total: usize, width: usize) -> String {
    if total == 0 {
        return format!("[{}]", " ".repeat(width));
    }
    let filled = (done * width) / total;
    let empty = width - filled;
    format!("[{}{}]", "#".repeat(filled), "-".repeat(empty))
}
