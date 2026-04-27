//! Export tasks to formats that round-trip back through `tc import`.
//!
//! Round-trip contract: every field in the import schema (`JsonRecord` for
//! JSON, `parse_kairo_md` for markdown) is preserved by export. Re-importing
//! is deduplicated via the `Imported from: <source_ref>` marker -- we set
//! `source_ref` to the original task ID so re-importing into the same store
//! is a no-op.
//!
//! Lossy by design (these don't fit the import schema):
//! - `id`, `status`, `created_at` -- import allocates fresh ones.
//! - `assignee`, `depends_on`, `files`, `pack_exclude` -- not part of
//!   `JsonRecord`; export would have nowhere for them to land back.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use serde::Serialize;
use tc_core::filter::Filter;
use tc_core::status::StatusMachine;
use tc_core::task::{Priority, Task};

use crate::cli::{ExportArgs, ExportFormat};
use crate::error::CliError;

pub fn run(args: ExportArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;
    let sm = StatusMachine::new(config.statuses);

    let query = args.query.join(" ");
    let filter =
        Filter::parse(&query).map_err(|e| CliError::user(format!("invalid filter: {e}")))?;

    let today = Local::now().date_naive();
    let selected: Vec<&Task> = tasks
        .iter()
        .filter(|t| {
            args.epic
                .as_ref()
                .is_none_or(|e| t.epic.eq_ignore_ascii_case(e))
        })
        .filter(|t| filter.matches(t, today))
        .collect();

    let rendered = match args.format {
        ExportFormat::Json => render_json(&selected)?,
        ExportFormat::Md => render_md(&selected, &sm, today),
    };

    match args.output {
        Some(path) => std::fs::write(&path, rendered)
            .map_err(|e| CliError::user(format!("write {}: {e}", path.display())))?,
        None => println!("{rendered}"),
    }

    Ok(())
}

// ── JSON ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonExport<'a> {
    title: &'a str,
    epic: &'a str,
    priority: Priority,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    tags: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    due: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduled: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none", with = "humantime_serde")]
    estimate: Option<Duration>,
    #[serde(skip_serializing_if = "str::is_empty")]
    notes: &'a str,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    acceptance_criteria: &'a [String],
    /// Original task ID. Re-imports into the same store hit the dedup
    /// path in `apply_imports` and skip silently.
    source_ref: &'a str,
}

fn render_json(tasks: &[&Task]) -> Result<String, CliError> {
    let records: Vec<JsonExport> = tasks
        .iter()
        .map(|t| JsonExport {
            title: &t.title,
            epic: &t.epic,
            priority: t.priority,
            tags: &t.tags,
            due: t.due,
            scheduled: t.scheduled,
            estimate: t.estimate,
            notes: &t.notes,
            acceptance_criteria: &t.acceptance_criteria,
            source_ref: &t.id.0,
        })
        .collect();

    serde_json::to_string_pretty(&records)
        .map_err(|e| CliError::user(format!("serialize JSON: {e}")))
}

// ── Markdown (kairo grammar) ──────────────────────────────────────────

fn render_md(tasks: &[&Task], sm: &StatusMachine, today: NaiveDate) -> String {
    let mut out = String::new();
    out.push_str(&format!("# tc tasks export -- {today}\n"));

    if tasks.is_empty() {
        out.push_str("\n_No tasks matched._\n");
        return out;
    }

    let grouped = group_by_epic(tasks);
    for (epic, epic_tasks) in &grouped {
        out.push_str(&format!("\n## {epic}\n\n"));
        for t in epic_tasks {
            write_task_md(&mut out, t, sm);
        }
    }

    out
}

fn write_task_md(out: &mut String, task: &Task, sm: &StatusMachine) {
    let checkbox = if sm.is_terminal(&task.status) {
        "- [x]"
    } else {
        "- [ ]"
    };

    out.push_str(checkbox);
    out.push(' ');
    out.push_str(&task.title);

    for tag in &task.tags {
        out.push_str(" #");
        out.push_str(tag);
    }

    // Skip P3 (the default) so re-import lands on the same value without
    // emitting a redundant token. P1/P2/P4/P5 are emitted explicitly.
    if task.priority != Priority::P3 {
        out.push_str(" !");
        out.push_str(&task.priority.to_string());
    }

    if let Some(due) = task.due {
        out.push_str(&format!(" due:{due}"));
    }
    if let Some(sched) = task.scheduled {
        out.push_str(&format!(" scheduled:{sched}"));
    }

    out.push('\n');

    for ac in &task.acceptance_criteria {
        out.push_str("  - [ ] ");
        out.push_str(ac);
        out.push('\n');
    }
}

fn group_by_epic<'a>(tasks: &[&'a Task]) -> BTreeMap<String, Vec<&'a Task>> {
    let mut map: BTreeMap<String, Vec<&'a Task>> = BTreeMap::new();
    for task in tasks {
        map.entry(task.epic.clone()).or_default().push(task);
    }
    map
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

    fn task(
        id: &str,
        title: &str,
        epic: &str,
        status: &str,
        priority: Priority,
        tags: &[&str],
    ) -> Task {
        Task {
            id: TaskId(id.into()),
            title: title.into(),
            epic: epic.into(),
            status: StatusId(status.into()),
            priority,
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
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

    #[test]
    fn json_export_emits_source_ref_as_id() {
        let t = task("T-007", "hello", "be", "todo", Priority::P3, &[]);
        let refs = vec![&t];
        let json = render_json(&refs).unwrap();
        assert!(json.contains("\"source_ref\": \"T-007\""), "{json}");
    }

    #[test]
    fn json_export_skips_default_and_empty_fields() {
        let t = task("T-001", "minimal", "be", "todo", Priority::P3, &[]);
        let refs = vec![&t];
        let json = render_json(&refs).unwrap();
        assert!(!json.contains("\"tags\""));
        assert!(!json.contains("\"due\""));
        assert!(!json.contains("\"scheduled\""));
        assert!(!json.contains("\"estimate\""));
        assert!(!json.contains("\"notes\""));
        assert!(!json.contains("\"acceptance_criteria\""));
    }

    #[test]
    fn json_export_includes_full_fields() {
        let mut t = task(
            "T-100",
            "rich",
            "auth",
            "todo",
            Priority::P1,
            &["backend", "perf"],
        );
        t.due = NaiveDate::from_ymd_opt(2026, 5, 1);
        t.scheduled = NaiveDate::from_ymd_opt(2026, 4, 28);
        t.estimate = Some(Duration::from_secs(2 * 3600));
        t.notes = "see RFC".into();
        t.acceptance_criteria = vec!["tests pass".into()];
        let refs = vec![&t];
        let json = render_json(&refs).unwrap();
        assert!(json.contains("\"priority\": \"p1\""), "{json}");
        assert!(json.contains("\"backend\""));
        assert!(json.contains("\"due\": \"2026-05-01\""));
        assert!(json.contains("\"scheduled\": \"2026-04-28\""));
        assert!(json.contains("\"estimate\": \"2h\""));
        assert!(json.contains("\"notes\": \"see RFC\""));
        assert!(json.contains("\"tests pass\""));
    }

    #[test]
    fn md_renders_with_grouping_and_tokens() {
        let mut t1 = task("T-001", "First", "auth", "todo", Priority::P1, &["backend"]);
        t1.due = NaiveDate::from_ymd_opt(2026, 5, 1);
        t1.acceptance_criteria = vec!["AC one".into(), "AC two".into()];

        let t2 = task("T-002", "Second", "docs", "done", Priority::P3, &[]);
        let refs = vec![&t1, &t2];

        let md = render_md(&refs, &sm(), NaiveDate::from_ymd_opt(2026, 4, 27).unwrap());

        assert!(md.contains("# tc tasks export -- 2026-04-27"));
        assert!(md.contains("## auth"));
        assert!(md.contains("## docs"));
        assert!(
            md.contains("- [ ] First #backend !p1 due:2026-05-01"),
            "{md}"
        );
        assert!(md.contains("  - [ ] AC one"));
        assert!(md.contains("  - [ ] AC two"));
        assert!(md.contains("- [x] Second"), "{md}");
        // Default priority (P3) suppressed
        assert!(!md.contains("Second !p3"));
    }

    #[test]
    fn md_empty_selection_emits_placeholder() {
        let md = render_md(&[], &sm(), NaiveDate::from_ymd_opt(2026, 4, 27).unwrap());
        assert!(md.contains("_No tasks matched._"));
    }
}
