//! Smart-view shortcuts (M-6.5).
//!
//! Each view filters open (non-terminal) tasks by date semantics and groups
//! the result by epic, matching `tc list` output. Views read "today" from
//! the local clock so users see what they'd expect from their wall calendar.
//!
//! - `today`: due or scheduled today.
//! - `upcoming`: due or scheduled within the next N days (default 7), exclusive of today.
//! - `overdue`: due before today.
//! - `inbox`: no due, no scheduled date (raw work waiting to be triaged).

use std::collections::BTreeMap;

use chrono::{Local, NaiveDate};
use tc_core::status::StatusMachine;
use tc_core::task::Task;

use crate::error::CliError;
use crate::output;

pub fn today() -> Result<(), CliError> {
    let today = Local::now().date_naive();
    run_view("Today", |t| {
        t.due == Some(today) || t.scheduled == Some(today)
    })
}

pub fn upcoming(days: u32) -> Result<(), CliError> {
    let today = Local::now().date_naive();
    let horizon = today
        .checked_add_days(chrono::Days::new(days as u64))
        .unwrap_or(today);

    let label = format!("Upcoming (next {days}d)");
    run_view(&label, move |t| {
        let in_window = |d: Option<NaiveDate>| d.is_some_and(|v| v > today && v <= horizon);
        in_window(t.due) || in_window(t.scheduled)
    })
}

pub fn overdue() -> Result<(), CliError> {
    let today = Local::now().date_naive();
    run_view("Overdue", move |t| t.due.is_some_and(|d| d < today))
}

pub fn inbox() -> Result<(), CliError> {
    run_view("Inbox", |t| t.due.is_none() && t.scheduled.is_none())
}

fn run_view<F>(label: &str, predicate: F) -> Result<(), CliError>
where
    F: Fn(&Task) -> bool,
{
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;

    if tasks.is_empty() {
        println!("No tasks. Run `tc add` to create one.");
        return Ok(());
    }

    let sm = StatusMachine::new(config.statuses);

    let filtered: Vec<&Task> = tasks
        .iter()
        .filter(|t| !sm.is_terminal(&t.status))
        .filter(|t| predicate(t))
        .collect();

    if filtered.is_empty() {
        println!("[{label}] -- nothing here.");
        return Ok(());
    }

    println!("[{label}]");
    for (epic, epic_tasks) in group_by_epic(&filtered) {
        println!("\n[{epic}]");
        println!("{}", output::format_task_refs(&epic_tasks));
    }

    Ok(())
}

fn group_by_epic<'a>(tasks: &[&'a Task]) -> BTreeMap<String, Vec<&'a Task>> {
    let mut map: BTreeMap<String, Vec<&'a Task>> = BTreeMap::new();
    for task in tasks {
        map.entry(task.epic.clone()).or_default().push(task);
    }
    map
}
