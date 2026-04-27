// Terminal output helpers (colors, formatting)

use tc_core::status::StatusId;
use tc_core::task::{Priority, Task};

// ANSI escape sequences
const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const WHITE: &str = "\x1b[37m";
const DIM: &str = "\x1b[2;37m";
const BOLD: &str = "\x1b[1m";

pub fn colors_enabled() -> bool {
    std::env::var("NO_COLOR").is_err()
}

fn colorize(color: &str, text: &str, use_color: bool) -> String {
    if use_color {
        format!("{color}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn status_color(status: &str) -> &'static str {
    match status {
        "todo" => WHITE,
        "in_progress" => YELLOW,
        "review" => BLUE,
        "done" => GREEN,
        "blocked" => RED,
        _ => WHITE,
    }
}

pub fn colored_status(status: &StatusId) -> String {
    colored_status_str(&status.0)
}

pub fn colored_status_str(status: &str) -> String {
    let use_color = colors_enabled();
    colorize(status_color(status), status, use_color)
}

fn priority_color(priority: &Priority) -> &'static str {
    match priority {
        Priority::P1 => RED,
        Priority::P2 => YELLOW,
        Priority::P3 => WHITE,
        Priority::P4 => BLUE,
        Priority::P5 => DIM,
    }
}

pub fn status_dot_color(status: &str) -> &'static str {
    match status {
        "todo" => "white",
        "in_progress" => "yellow",
        "review" => "lightblue",
        "done" => "green",
        "blocked" => "red",
        _ => "white",
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

pub fn print_success(msg: &str) {
    let c = colors_enabled();
    eprintln!("{}", colorize(GREEN, &format!("✓ {msg}"), c));
}

pub fn print_warning(msg: &str) {
    let c = colors_enabled();
    eprintln!("{}", colorize(YELLOW, &format!("⚠ {msg}"), c));
}

pub fn print_error(msg: &str) {
    let c = colors_enabled();
    eprintln!("{}", colorize(RED, &format!("✗ {msg}"), c));
}

// ── Table formatter for `tc list` ────────────────────────────────────

struct ColumnWidths {
    id: usize,
    title: usize,
    epic: usize,
    status: usize,
    priority: usize,
}

fn compute_widths(tasks: &[Task]) -> ColumnWidths {
    let min = |field_max: usize, min_w: usize| field_max.max(min_w);
    ColumnWidths {
        id: min(tasks.iter().map(|t| t.id.0.len()).max().unwrap_or(0), 2),
        title: min(tasks.iter().map(|t| t.title.len()).max().unwrap_or(0), 5),
        epic: min(tasks.iter().map(|t| t.epic.len()).max().unwrap_or(0), 4),
        status: min(tasks.iter().map(|t| t.status.0.len()).max().unwrap_or(0), 6),
        priority: min(
            tasks
                .iter()
                .map(|t| t.priority.to_string().len())
                .max()
                .unwrap_or(0),
            8,
        ),
    }
}

fn format_row(
    id: &str,
    title: &str,
    epic: &str,
    status: &str,
    priority: &Priority,
    w: &ColumnWidths,
    use_color: bool,
) -> String {
    let status_cell = {
        let padded = format!("{:<w$}", status, w = w.status);
        if use_color {
            colorize(status_color(status), &padded, true)
        } else {
            padded
        }
    };
    let priority_str = priority.to_string();
    let priority_cell = {
        let padded = format!("{:<w$}", priority_str, w = w.priority);
        if use_color {
            colorize(priority_color(priority), &padded, true)
        } else {
            padded
        }
    };
    format!(
        "{:<wi$}  {:<wt$}  {:<we$}  {}  {}",
        id,
        title,
        epic,
        status_cell,
        priority_cell,
        wi = w.id,
        wt = w.title,
        we = w.epic,
    )
}

pub fn format_task_refs(tasks: &[&Task]) -> String {
    let owned: Vec<Task> = tasks.iter().map(|t| (*t).clone()).collect();
    format_table_impl(&owned, colors_enabled())
}

fn format_table_impl(tasks: &[Task], use_color: bool) -> String {
    if tasks.is_empty() {
        return String::new();
    }

    let w = compute_widths(tasks);

    let header = {
        let raw = format!(
            "{:<wi$}  {:<wt$}  {:<we$}  {:<ws$}  {:<wp$}",
            "ID",
            "Title",
            "Epic",
            "Status",
            "Priority",
            wi = w.id,
            wt = w.title,
            we = w.epic,
            ws = w.status,
            wp = w.priority,
        );
        if use_color {
            colorize(BOLD, &raw, true)
        } else {
            raw
        }
    };

    let separator = format!(
        "{:-<wi$}  {:-<wt$}  {:-<we$}  {:-<ws$}  {:-<wp$}",
        "",
        "",
        "",
        "",
        "",
        wi = w.id,
        wt = w.title,
        we = w.epic,
        ws = w.status,
        wp = w.priority,
    );

    let body = tasks.iter().map(|t| {
        format_row(
            &t.id.0,
            &t.title,
            &t.epic,
            &t.status.0,
            &t.priority,
            &w,
            use_color,
        )
    });

    let mut lines = vec![header, separator];
    lines.extend(body);
    lines.join("\n")
}

// ── Detail formatter for `tc show` ──────────────────────────────────

pub fn format_detail(task: &Task) -> String {
    format_detail_impl(task, colors_enabled())
}

fn format_detail_impl(task: &Task, use_color: bool) -> String {
    let bold_str = |s: &str| colorize(BOLD, s, use_color);
    let status_str = colorize(status_color(&task.status.0), &task.status.0, use_color);

    let assignee = task
        .assignee
        .as_ref()
        .map(|a| format!("{a:?}"))
        .unwrap_or_else(|| "--".to_string());

    let priority_str = {
        let p = task.priority.to_string();
        colorize(priority_color(&task.priority), &p, use_color)
    };

    let mut lines = vec![
        format!(
            "{} {}",
            bold_str(&task.id.to_string()),
            bold_str(&task.title)
        ),
        format!("Epic:       {}", task.epic),
        format!("Status:     {status_str}"),
        format!("Priority:   {priority_str}"),
        format!("Assignee:   {assignee}"),
        format!("Created:    {}", task.created_at.format("%Y-%m-%d %H:%M")),
    ];

    if !task.tags.is_empty() {
        lines.push(format!("Tags:       {}", task.tags.join(", ")));
    }

    if let Some(due) = task.due {
        lines.push(format!("Due:        {due}"));
    }

    if let Some(scheduled) = task.scheduled {
        lines.push(format!("Scheduled:  {scheduled}"));
    }

    if let Some(estimate) = task.estimate {
        lines.push(format!(
            "Estimate:   {}",
            humantime::format_duration(estimate)
        ));
    }

    if !task.depends_on.is_empty() {
        let deps: Vec<String> = task.depends_on.iter().map(|d| d.to_string()).collect();
        lines.push(format!("Depends on: {}", deps.join(", ")));
    }

    if !task.files.is_empty() {
        lines.push(format!("Files:      {}", task.files.join(", ")));
    }

    if !task.acceptance_criteria.is_empty() {
        lines.push(String::new());
        lines.push(bold_str("Acceptance Criteria:").to_string());
        for ac in &task.acceptance_criteria {
            lines.push(format!("  - {ac}"));
        }
    }

    if !task.notes.is_empty() {
        lines.push(String::new());
        lines.push(bold_str("Notes:").to_string());
        lines.push(task.notes.clone());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tc_core::task::{Assignee, Priority, Task, TaskId};

    fn make_task(id: &str, title: &str, epic: &str, status: &str) -> Task {
        Task {
            id: TaskId(id.to_string()),
            title: title.to_string(),
            epic: epic.to_string(),
            status: StatusId(status.to_string()),
            priority: Priority::default(),
            tags: vec![],
            due: None,
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

    // ── colored_status ───────────────────────────────────────────────

    #[test]
    fn status_color_mapping() {
        assert_eq!(status_color("todo"), WHITE);
        assert_eq!(status_color("in_progress"), YELLOW);
        assert_eq!(status_color("review"), BLUE);
        assert_eq!(status_color("done"), GREEN);
        assert_eq!(status_color("blocked"), RED);
        assert_eq!(status_color("custom"), WHITE);
    }

    #[test]
    fn colorize_disabled_returns_plain() {
        assert_eq!(colorize(GREEN, "hello", false), "hello");
    }

    #[test]
    fn colorize_enabled_wraps_ansi() {
        let result = colorize(GREEN, "hello", true);
        assert!(result.starts_with(GREEN));
        assert!(result.ends_with(RESET));
        assert!(result.contains("hello"));
    }

    // ── format_table ─────────────────────────────────────────────────

    #[test]
    fn table_empty() {
        assert_eq!(format_table_impl(&[], false), "");
    }

    #[test]
    fn table_single_task() {
        let tasks = vec![make_task("T-001", "My Task", "backend", "todo")];
        let result = format_table_impl(&tasks, false);
        assert!(result.contains("T-001"));
        assert!(result.contains("My Task"));
        assert!(result.contains("backend"));
        assert!(result.contains("todo"));
        assert!(result.contains("ID"));
        assert!(result.contains("Title"));
        assert!(result.contains("Epic"));
        assert!(result.contains("Status"));
        assert!(result.contains("Priority"));
        assert!(result.contains("p3"));
    }

    #[test]
    fn table_row_count() {
        let tasks = vec![
            make_task("T-001", "First Task", "backend", "todo"),
            make_task("T-002", "Second Task", "frontend", "in_progress"),
            make_task("T-003", "Third Task", "backend", "done"),
        ];
        let result = format_table_impl(&tasks, false);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 5); // header + separator + 3 rows
    }

    #[test]
    fn table_columns_aligned() {
        let tasks = vec![
            make_task("T-001", "Short", "be", "todo"),
            make_task(
                "T-002",
                "A Much Longer Title",
                "frontend-app",
                "in_progress",
            ),
        ];
        let result = format_table_impl(&tasks, false);
        let lines: Vec<&str> = result.lines().collect();
        let header_title_pos = lines[0].find("Title").expect("Title header");
        let row_title_pos = lines[2].find("Short").expect("Short in row");
        assert_eq!(header_title_pos, row_title_pos);
    }

    #[test]
    fn table_colored_vs_plain_no_color() {
        let tasks = vec![
            make_task("T-001", "First", "be", "todo"),
            make_task("T-002", "Second", "fe", "done"),
        ];
        let plain = format_table_impl(&tasks, false);
        // Plain should contain no ANSI escapes
        assert!(!plain.contains("\x1b["));
    }

    #[test]
    fn table_colored_contains_ansi() {
        let tasks = vec![make_task("T-001", "Task", "be", "done")];
        let colored = format_table_impl(&tasks, true);
        assert!(colored.contains("\x1b["));
    }

    // ── format_detail ────────────────────────────────────────────────

    #[test]
    fn detail_basic_fields() {
        let task = make_task("T-001", "My Task", "backend", "todo");
        let result = format_detail_impl(&task, false);
        assert!(result.contains("T-001 My Task"));
        assert!(result.contains("Epic:       backend"));
        assert!(result.contains("Status:     todo"));
        assert!(result.contains("Priority:   p3"));
        assert!(result.contains("Assignee:   --"));
    }

    #[test]
    fn detail_with_deps_and_files() {
        let mut task = make_task("T-002", "Dep Task", "fe", "in_progress");
        task.depends_on = vec![TaskId("T-001".to_string())];
        task.files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let result = format_detail_impl(&task, false);
        assert!(result.contains("Depends on: T-001"));
        assert!(result.contains("Files:      src/main.rs, src/lib.rs"));
    }

    #[test]
    fn detail_with_notes() {
        let mut task = make_task("T-003", "Task", "test", "blocked");
        task.notes = "API not ready".to_string();
        let result = format_detail_impl(&task, false);
        assert!(result.contains("Notes:"));
        assert!(result.contains("API not ready"));
    }

    #[test]
    fn detail_with_assignee() {
        let mut task = make_task("T-001", "Task", "be", "todo");
        task.assignee = Some(Assignee::Claude);
        let result = format_detail_impl(&task, false);
        assert!(result.contains("Assignee:   Claude"));
    }

    #[test]
    fn detail_colored_contains_ansi() {
        let task = make_task("T-001", "Task", "be", "done");
        let result = format_detail_impl(&task, true);
        assert!(result.contains("\x1b["));
    }

    #[test]
    fn detail_plain_no_ansi() {
        let task = make_task("T-001", "Task", "be", "done");
        let result = format_detail_impl(&task, false);
        assert!(!result.contains("\x1b["));
    }

    #[test]
    fn detail_with_acceptance_criteria() {
        let mut task = make_task("T-001", "Task", "be", "todo");
        task.acceptance_criteria = vec!["API returns 200".into(), "Tests pass".into()];
        let result = format_detail_impl(&task, false);
        assert!(result.contains("Acceptance Criteria:"));
        assert!(result.contains("- API returns 200"));
        assert!(result.contains("- Tests pass"));
    }

    #[test]
    fn detail_without_acceptance_criteria() {
        let task = make_task("T-001", "Task", "be", "todo");
        let result = format_detail_impl(&task, false);
        assert!(!result.contains("Acceptance Criteria"));
    }
}
