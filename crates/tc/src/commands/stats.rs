use std::collections::BTreeMap;

use tc_core::status::StatusMachine;

use crate::error::CliError;
use crate::output;

pub fn run() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;

    if tasks.is_empty() {
        println!("No tasks.");
        return Ok(());
    }

    let sm = StatusMachine::new(config.statuses);
    let total = tasks.len();
    let mut done = 0;
    let mut epics: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut by_status: BTreeMap<&str, usize> = BTreeMap::new();

    for task in &tasks {
        let is_terminal = sm.is_terminal(&task.status);
        if is_terminal {
            done += 1;
        }
        let entry = epics.entry(&task.epic).or_insert((0, 0));
        entry.0 += 1;
        if is_terminal {
            entry.1 += 1;
        }
        *by_status.entry(&task.status.0).or_insert(0) += 1;
    }

    println!("Overall: {done}/{total} done ({}%)", done * 100 / total);
    println!("{}", progress_bar(done, total, 30));
    println!();

    println!("By status:");
    for (status, count) in &by_status {
        println!("  {:<15} {}", output::colored_status_str(status), count);
    }
    println!();

    println!("By epic:");
    for (epic, (epic_total, epic_done)) in &epics {
        let pct = if *epic_total > 0 {
            epic_done * 100 / epic_total
        } else {
            0
        };
        println!(
            "  {:<15} {}/{} ({}%) {}",
            epic,
            epic_done,
            epic_total,
            pct,
            progress_bar(*epic_done, *epic_total, 20)
        );
    }

    Ok(())
}

fn progress_bar(done: usize, total: usize, width: usize) -> String {
    if total == 0 {
        return format!("[{}]", " ".repeat(width));
    }
    let filled = (done * width) / total;
    let empty = width - filled;
    format!("[{}{}]", "#".repeat(filled), "-".repeat(empty))
}
