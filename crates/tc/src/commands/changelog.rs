use std::collections::BTreeMap;

use tc_core::status::StatusMachine;
use tc_core::task::Task;

use crate::cli::{ChangelogArgs, ChangelogFormat};
use crate::error::CliError;

pub fn run(args: ChangelogArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;
    let sm = StatusMachine::new(config.statuses);

    let done_tasks: Vec<&Task> = tasks
        .iter()
        .filter(|t| sm.is_terminal(&t.status))
        .filter(|t| {
            args.epic
                .as_ref()
                .is_none_or(|e| t.epic.eq_ignore_ascii_case(e))
        })
        .collect();

    if done_tasks.is_empty() {
        println!("No completed tasks.");
        return Ok(());
    }

    let grouped = group_by_epic(&done_tasks);
    let output = match args.format {
        ChangelogFormat::Markdown => format_markdown(&grouped),
        ChangelogFormat::Plain => format_plain(&grouped),
    };

    println!("{output}");
    Ok(())
}

fn group_by_epic<'a>(tasks: &[&'a Task]) -> BTreeMap<&'a str, Vec<&'a Task>> {
    let mut map: BTreeMap<&'a str, Vec<&'a Task>> = BTreeMap::new();
    for task in tasks {
        map.entry(&task.epic).or_default().push(task);
    }
    map
}

fn format_markdown(grouped: &BTreeMap<&str, Vec<&Task>>) -> String {
    let mut lines = vec!["# Changelog".to_string(), String::new()];

    for (epic, tasks) in grouped {
        lines.push(format!("## {epic}"));
        lines.push(String::new());
        for task in tasks {
            let entry = format!("- **{}**: {}", task.id, task.title);
            lines.push(entry);
        }
        lines.push(String::new());
    }

    // Remove trailing blank line
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn format_plain(grouped: &BTreeMap<&str, Vec<&Task>>) -> String {
    let mut lines = vec![
        "Changelog".to_string(),
        "=========".to_string(),
        String::new(),
    ];

    for (epic, tasks) in grouped {
        lines.push(format!("[{epic}]"));
        for task in tasks {
            lines.push(format!("  {} - {}", task.id, task.title));
        }
        lines.push(String::new());
    }

    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tc_core::status::StatusId;
    use tc_core::task::TaskId;

    fn make_task(id: &str, title: &str, epic: &str, status: &str) -> Task {
        Task {
            id: TaskId(id.to_string()),
            title: title.to_string(),
            epic: epic.to_string(),
            status: StatusId(status.to_string()),
            priority: Default::default(),
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
    fn markdown_single_epic() {
        let tasks = [
            make_task("T-001", "Add login", "auth", "done"),
            make_task("T-002", "Add logout", "auth", "done"),
        ];
        let refs: Vec<&Task> = tasks.iter().collect();
        let grouped = group_by_epic(&refs);
        let result = format_markdown(&grouped);

        assert!(result.starts_with("# Changelog"));
        assert!(result.contains("## auth"));
        assert!(result.contains("- **T-001**: Add login"));
        assert!(result.contains("- **T-002**: Add logout"));
    }

    #[test]
    fn markdown_multiple_epics_sorted() {
        let tasks = [
            make_task("T-001", "Task A", "beta", "done"),
            make_task("T-002", "Task B", "alpha", "done"),
        ];
        let refs: Vec<&Task> = tasks.iter().collect();
        let grouped = group_by_epic(&refs);
        let result = format_markdown(&grouped);

        let alpha_pos = result.find("## alpha").expect("alpha section");
        let beta_pos = result.find("## beta").expect("beta section");
        assert!(
            alpha_pos < beta_pos,
            "epics should be sorted alphabetically"
        );
    }

    #[test]
    fn plain_format() {
        let tasks = [make_task("T-001", "Fix bug", "core", "done")];
        let refs: Vec<&Task> = tasks.iter().collect();
        let grouped = group_by_epic(&refs);
        let result = format_plain(&grouped);

        assert!(result.starts_with("Changelog"));
        assert!(result.contains("[core]"));
        assert!(result.contains("T-001 - Fix bug"));
    }

    #[test]
    fn plain_no_trailing_blank() {
        let tasks = [make_task("T-001", "Task", "epic", "done")];
        let refs: Vec<&Task> = tasks.iter().collect();
        let grouped = group_by_epic(&refs);
        let result = format_plain(&grouped);

        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn markdown_no_trailing_blank() {
        let tasks = [make_task("T-001", "Task", "epic", "done")];
        let refs: Vec<&Task> = tasks.iter().collect();
        let grouped = group_by_epic(&refs);
        let result = format_markdown(&grouped);

        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn group_by_epic_groups_correctly() {
        let tasks = [
            make_task("T-001", "A", "x", "done"),
            make_task("T-002", "B", "y", "done"),
            make_task("T-003", "C", "x", "done"),
        ];
        let refs: Vec<&Task> = tasks.iter().collect();
        let grouped = group_by_epic(&refs);

        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped["x"].len(), 2);
        assert_eq!(grouped["y"].len(), 1);
    }
}
