use std::collections::BTreeMap;

use chrono::{Local, NaiveDate};
use tc_core::status::StatusMachine;
use tc_core::task::{Priority, Task};

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

    let priority_section = format_priority_section(&tasks, &sm);
    if !priority_section.is_empty() {
        println!();
        print!("{priority_section}");
    }

    let today = Local::now().date_naive();
    let today_section = format_today_section(&tasks, &sm, today);
    if !today_section.is_empty() {
        println!();
        print!("{today_section}");
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

/// Burndown by priority. Skips priorities with zero tasks so the section
/// stays useful in small workspaces. Returns "" when no priority has any
/// task -- caller suppresses the heading.
fn format_priority_section(tasks: &[Task], sm: &StatusMachine) -> String {
    let counts = priority_counts(tasks, sm);
    if counts.iter().all(|(_, total, _)| *total == 0) {
        return String::new();
    }
    let mut out = String::from("By priority:\n");
    for (p, total, done) in counts {
        if total == 0 {
            continue;
        }
        let pct = done * 100 / total;
        out.push_str(&format!(
            "  {:<5} {}/{} ({}%) {}\n",
            p.to_string(),
            done,
            total,
            pct,
            progress_bar(done, total, 20),
        ));
    }
    out
}

fn priority_counts(tasks: &[Task], sm: &StatusMachine) -> Vec<(Priority, usize, usize)> {
    [
        Priority::P1,
        Priority::P2,
        Priority::P3,
        Priority::P4,
        Priority::P5,
    ]
    .iter()
    .map(|p| {
        let total = tasks.iter().filter(|t| t.priority == *p).count();
        let done = tasks
            .iter()
            .filter(|t| t.priority == *p && sm.is_terminal(&t.status))
            .count();
        (*p, total, done)
    })
    .collect()
}

/// "Today's plate" -- tasks whose `due` or `scheduled` falls on `today`.
/// Done count uses the StatusMachine's terminal predicate, so workspaces
/// with custom statuses still get accurate progress. Returns "" when the
/// plate is empty (suppresses noise on idle days).
fn format_today_section(tasks: &[Task], sm: &StatusMachine, today: NaiveDate) -> String {
    let plate: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.due == Some(today) || t.scheduled == Some(today))
        .collect();

    if plate.is_empty() {
        return String::new();
    }

    let total = plate.len();
    let done = plate.iter().filter(|t| sm.is_terminal(&t.status)).count();
    let remaining = total - done;
    let pct = done * 100 / total;

    let mut out = format!("Today ({today}):\n");
    out.push_str(&format!("  scheduled or due:  {total}\n"));
    out.push_str(&format!("  done:              {done}\n"));
    out.push_str(&format!(
        "  remaining:         {remaining}   ({pct}% complete)\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tc_core::status::{StatusDef, StatusId};
    use tc_core::task::TaskId;

    fn sm() -> StatusMachine {
        StatusMachine::new(vec![
            StatusDef {
                id: StatusId("todo".into()),
                label: "Todo".into(),
                terminal: false,
                active: false,
            },
            StatusDef {
                id: StatusId("done".into()),
                label: "Done".into(),
                terminal: true,
                active: false,
            },
        ])
    }

    fn task(id: &str, status: &str, priority: Priority, due: Option<NaiveDate>) -> Task {
        Task {
            id: TaskId(id.into()),
            title: format!("Task {id}"),
            epic: "default".into(),
            status: StatusId(status.into()),
            priority,
            tags: vec![],
            due,
            scheduled: None,
            estimate: None,
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn priority_breakdown_counts_correctly() {
        let tasks = vec![
            task("T-001", "todo", Priority::P1, None),
            task("T-002", "done", Priority::P1, None),
            task("T-003", "done", Priority::P2, None),
            task("T-004", "todo", Priority::P3, None),
            task("T-005", "done", Priority::P3, None),
            task("T-006", "done", Priority::P3, None),
        ];
        let counts = priority_counts(&tasks, &sm());
        // (Priority, total, done)
        assert_eq!(counts[0], (Priority::P1, 2, 1));
        assert_eq!(counts[1], (Priority::P2, 1, 1));
        assert_eq!(counts[2], (Priority::P3, 3, 2));
        assert_eq!(counts[3], (Priority::P4, 0, 0));
        assert_eq!(counts[4], (Priority::P5, 0, 0));
    }

    #[test]
    fn priority_section_skips_empty_buckets() {
        let tasks = vec![
            task("T-001", "todo", Priority::P1, None),
            task("T-002", "done", Priority::P1, None),
        ];
        let s = format_priority_section(&tasks, &sm());
        assert!(s.contains("p1    1/2"), "{s}");
        assert!(!s.contains("p2"), "should hide empty p2: {s}");
        assert!(!s.contains("p3"));
    }

    #[test]
    fn priority_section_empty_when_no_tasks() {
        let s = format_priority_section(&[], &sm());
        assert!(s.is_empty());
    }

    #[test]
    fn today_section_uses_due_or_scheduled() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let tomorrow = NaiveDate::from_ymd_opt(2026, 4, 28).unwrap();
        let mut t_due = task("T-001", "todo", Priority::P3, Some(today));
        t_due.scheduled = None;

        let mut t_sched = task("T-002", "todo", Priority::P3, None);
        t_sched.scheduled = Some(today);

        let t_unrelated = task("T-003", "todo", Priority::P3, Some(tomorrow));
        let t_undated = task("T-004", "todo", Priority::P3, None);

        let s = format_today_section(&[t_due, t_sched, t_unrelated, t_undated], &sm(), today);
        assert!(s.contains("scheduled or due:  2"), "{s}");
    }

    #[test]
    fn today_section_done_count_uses_status_machine() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let tasks = vec![
            task("T-001", "done", Priority::P3, Some(today)),
            task("T-002", "todo", Priority::P3, Some(today)),
            task("T-003", "todo", Priority::P3, Some(today)),
        ];
        let s = format_today_section(&tasks, &sm(), today);
        assert!(s.contains("scheduled or due:  3"));
        assert!(s.contains("done:              1"));
        assert!(s.contains("remaining:         2"));
        assert!(s.contains("33% complete"));
    }

    #[test]
    fn today_section_omitted_when_plate_empty() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let tomorrow = NaiveDate::from_ymd_opt(2026, 4, 28).unwrap();
        let tasks = vec![task("T-001", "todo", Priority::P3, Some(tomorrow))];
        let s = format_today_section(&tasks, &sm(), today);
        assert!(s.is_empty(), "expected empty, got: {s}");
    }
}
