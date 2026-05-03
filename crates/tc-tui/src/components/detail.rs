use std::time::Duration;

use chrono::{Local, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tc_core::status::StatusMachine;
use tc_core::task::{Task, TaskId};

use crate::app::{App, FocusPanel};
use crate::theme::Palette;

const LIVE_TAIL_LINES: usize = 6;
const INLINE_DAG_BUDGET: usize = 4;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.focus == FocusPanel::Detail;
    let title = if focused { "[ Detail ]" } else { " Detail " };
    let border_color = if focused {
        app.palette.border_focused
    } else {
        app.palette.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let Some(task) = app.selected_task() else {
        frame.render_widget(Paragraph::new("(no task selected)").block(block), area);
        return;
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Live tail of the worker log -- only when a worker exists for this
    // task. The tail body shrinks the metadata pane proportionally.
    let has_worker = app.worker_for(&task.id).is_some();
    let tail_height = if has_worker {
        (LIVE_TAIL_LINES as u16 + 2).min(inner.height.saturating_sub(4))
    } else {
        0
    };

    let chunks = if has_worker && tail_height > 0 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(tail_height)])
            .split(inner)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(inner)
    };

    render_metadata(app, &task, frame, chunks[0]);
    if has_worker && chunks.len() > 1 {
        render_live_tail(app, &task, frame, chunks[1]);
    }
}

fn render_metadata(app: &App, task: &Task, frame: &mut Frame<'_>, area: Rect) {
    let palette = &app.palette;
    let today = Local::now().date_naive();
    let mut lines: Vec<Line<'static>> = vec![
        label_value("ID", &task.id.0, palette),
        label_value("Title", &task.title, palette),
        label_value("Epic", &task.epic, palette),
        Line::from(vec![
            muted_label("Status", palette),
            Span::styled(
                task.status.0.clone(),
                Style::default().fg(palette.status(&task.status.0)),
            ),
        ]),
        Line::from(vec![
            muted_label("Priority", palette),
            Span::styled(
                task.priority.to_string().to_uppercase(),
                Style::default()
                    .fg(palette.priority(task.priority))
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if !task.tags.is_empty() {
        let mut spans: Vec<Span<'static>> = vec![muted_label("Tags", palette)];
        'tags: for (i, tag) in task.tags.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("·{tag}"),
                Style::default().fg(palette.tag),
            ));
            continue 'tags;
        }
        lines.push(Line::from(spans));
    }

    if let Some(due) = task.due {
        lines.push(due_line("Due", due, today, palette));
    }
    if let Some(scheduled) = task.scheduled {
        lines.push(due_line("Scheduled", scheduled, today, palette));
    }
    if let Some(estimate) = task.estimate {
        lines.push(label_value("Estimate", &format_duration(estimate), palette));
    }

    if !task.files.is_empty() {
        lines.push(label_value("Files", &task.files.join(", "), palette));
    }

    let token_estimate = estimate_tokens(&task.notes, &task.files);
    lines.push(label_value(
        "Context",
        &format!("~{token_estimate} tokens"),
        palette,
    ));

    let inline_dag = inline_dag_preview(app, &task.id);
    if !inline_dag.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Dependencies",
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        )));
        lines.extend(inline_dag);
    }

    if !task.acceptance_criteria.is_empty() {
        lines.push(Line::from(""));
        let header = ac_header(task, &app.status_machine, palette);
        lines.push(header);
        let detail_focused = app.focus == FocusPanel::Detail;
        let cursor = app.selected_ac.min(task.acceptance_criteria.len() - 1);
        let was_terminal = app.status_machine.is_terminal(&task.status);
        'ac: for (i, criterion) in task.acceptance_criteria.iter().enumerate() {
            let is_cursor = detail_focused && i == cursor;
            lines.push(ac_row(criterion, was_terminal, is_cursor, palette));
            continue 'ac;
        }
        if detail_focused {
            lines.push(Line::from(Span::styled(
                "  (space to toggle, j/k to move)",
                Style::default().fg(palette.muted),
            )));
        }
    }

    if !task.notes.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Notes",
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        )));
        'notes: for note_line in task.notes.lines() {
            lines.push(Line::from(note_line.to_string()));
            continue 'notes;
        }
    }

    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn render_live_tail(app: &App, task: &Task, frame: &mut Frame<'_>, area: Rect) {
    let palette = &app.palette;
    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        " Live worker output ",
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(worker) = app.worker_for(&task.id) {
        spans.push(Span::styled(
            format!("[{}] ", worker.status),
            Style::default().fg(palette.muted),
        ));
    }

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(palette.border))
        .title(Line::from(spans));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line<'static>> = recent_log_lines(app, &task.id, LIVE_TAIL_LINES)
        .into_iter()
        .map(|l| Line::from(Span::styled(l, Style::default().fg(palette.muted))))
        .collect();
    let placeholder = lines.is_empty();
    let body: Vec<Line<'static>> = if placeholder {
        vec![Line::from(Span::styled(
            "(no output yet)",
            Style::default().fg(palette.muted),
        ))]
    } else {
        lines
    };
    frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: true }), inner);
}

/// Pull the last `n` non-empty lines from the in-memory `LogView`. Returns
/// an empty slice when the buffer is for a different task -- this protects
/// against showing stale output for a moment after the selection moves to
/// another task before the next tail tick lands.
fn recent_log_lines(app: &App, task_id: &TaskId, n: usize) -> Vec<String> {
    if app.log_view.lines.is_empty() {
        return Vec::new();
    }
    if app.log_view.task.as_ref() != Some(task_id) {
        return Vec::new();
    }
    app.log_view
        .lines
        .iter()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(n)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Render up to `INLINE_DAG_BUDGET` direct dependencies and dependents,
/// each with its current status colored. Anything beyond the budget is
/// summarized as `(+N more)` so the panel never blows past the visible
/// rows.
fn inline_dag_preview(app: &App, id: &TaskId) -> Vec<Line<'static>> {
    let palette = &app.palette;
    let by_id: std::collections::HashMap<&TaskId, &Task> =
        app.tasks.iter().map(|t| (&t.id, t)).collect();
    let mut lines: Vec<Line<'static>> = Vec::new();

    let deps = app.dag.dependencies(id);
    if deps.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ↑ depends on (none)",
            Style::default().fg(palette.muted),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  ↑ depends on:",
            Style::default().fg(palette.muted),
        )));
        let shown = deps.iter().take(INLINE_DAG_BUDGET);
        'deps: for dep in shown {
            lines.push(dep_line(dep, &by_id, palette));
            continue 'deps;
        }
        if deps.len() > INLINE_DAG_BUDGET {
            lines.push(Line::from(Span::styled(
                format!("    (+{} more)", deps.len() - INLINE_DAG_BUDGET),
                Style::default().fg(palette.muted),
            )));
        }
    }

    let dependents = app.dag.dependents(id);
    if !dependents.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ↓ blocks:",
            Style::default().fg(palette.muted),
        )));
        'dependents: for dep in dependents.iter().take(INLINE_DAG_BUDGET) {
            lines.push(dep_line(dep, &by_id, palette));
            continue 'dependents;
        }
        if dependents.len() > INLINE_DAG_BUDGET {
            lines.push(Line::from(Span::styled(
                format!("    (+{} more)", dependents.len() - INLINE_DAG_BUDGET),
                Style::default().fg(palette.muted),
            )));
        }
    }
    lines
}

fn dep_line(
    id: &TaskId,
    by_id: &std::collections::HashMap<&TaskId, &Task>,
    palette: &Palette,
) -> Line<'static> {
    let (status_label, status_color) = match by_id.get(id) {
        Some(t) => (t.status.0.clone(), palette.status(&t.status.0)),
        None => ("?".to_string(), palette.muted),
    };
    let title = by_id
        .get(id)
        .map(|t| t.title.clone())
        .unwrap_or_else(|| "(missing)".to_string());
    Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{} ", id.0),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{status_label}] "),
            Style::default().fg(status_color),
        ),
        Span::styled(title, Style::default().fg(palette.muted)),
    ])
}

fn ac_header(task: &Task, sm: &StatusMachine, palette: &Palette) -> Line<'static> {
    let total = task.acceptance_criteria.len();
    let was_terminal = sm.is_terminal(&task.status);
    let completed = task
        .acceptance_criteria
        .iter()
        .filter(|c| parse_ac_state(c, was_terminal).0)
        .count();
    Line::from(vec![
        Span::styled(
            "Acceptance Criteria",
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("({completed}/{total})"),
            Style::default().fg(palette.muted),
        ),
    ])
}

fn ac_row(criterion: &str, was_terminal: bool, cursor: bool, palette: &Palette) -> Line<'static> {
    let (checked, body) = parse_ac_state(criterion, was_terminal);
    let (mark, color) = if checked {
        ("[x] ", palette.status_done)
    } else {
        ("[ ] ", palette.muted)
    };
    let mut text_style = Style::default();
    if checked {
        text_style = text_style.add_modifier(Modifier::CROSSED_OUT);
    }
    let prefix = if cursor { "> " } else { "  " };
    let prefix_style = if cursor {
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    if cursor {
        text_style = text_style.add_modifier(Modifier::BOLD);
    }
    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(mark, Style::default().fg(color)),
        Span::styled(body, text_style),
    ])
}

/// Parse the leading `[x]` / `[ ]` markdown checkbox out of an acceptance
/// criterion string. Falls back to `default_when_unmarked` (the task's
/// terminal status) when no explicit prefix is present so legacy tasks
/// keep working: a `done` task still renders with all AC checked, a `todo`
/// task still renders with all AC unchecked.
pub fn parse_ac_state(criterion: &str, default_when_unmarked: bool) -> (bool, String) {
    if let Some(rest) = criterion
        .strip_prefix("[x] ")
        .or_else(|| criterion.strip_prefix("[X] "))
    {
        return (true, rest.to_string());
    }
    if let Some(rest) = criterion.strip_prefix("[ ] ") {
        return (false, rest.to_string());
    }
    (default_when_unmarked, criterion.to_string())
}

/// Encode a checked / unchecked state back into the canonical
/// `[x] body` / `[ ] body` form used on disk.
pub fn write_ac_state(body: &str, checked: bool) -> String {
    if checked {
        format!("[x] {body}")
    } else {
        format!("[ ] {body}")
    }
}

fn label_value(label: &str, value: &str, palette: &Palette) -> Line<'static> {
    Line::from(vec![
        muted_label(label, palette),
        Span::raw(value.to_string()),
    ])
}

fn muted_label(label: &str, palette: &Palette) -> Span<'static> {
    Span::styled(format!("{label:<10}"), Style::default().fg(palette.muted))
}

fn due_line(label: &str, date: NaiveDate, today: NaiveDate, palette: &Palette) -> Line<'static> {
    let (text, color) = if date < today {
        (format!("{date} (overdue)"), palette.due_overdue)
    } else if date == today {
        (format!("{date} (today)"), palette.due_today)
    } else {
        (date.to_string(), palette.due_future)
    };
    Line::from(vec![
        muted_label(label, palette),
        Span::styled(text, Style::default().fg(color)),
    ])
}

fn estimate_tokens(notes: &str, files: &[String]) -> usize {
    notes.len() / 4 + files.iter().map(|f| f.len() / 4).sum::<usize>()
}

fn format_duration(d: Duration) -> String {
    let total = d.as_secs();
    if total == 0 {
        return "0s".to_string();
    }
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    let mut parts: Vec<String> = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 && hours == 0 {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{app_with, dummy_task};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tc_core::task::TaskId;
    use tc_spawn::process::{WorkerState, WorkerStatus};

    #[test]
    fn format_duration_compact() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(60)), "1m");
        assert_eq!(format_duration(Duration::from_secs(3 * 3600)), "3h");
        assert_eq!(
            format_duration(Duration::from_secs(2 * 3600 + 30 * 60)),
            "2h 30m"
        );
    }

    #[test]
    fn estimate_tokens_counts_notes_and_files() {
        assert_eq!(estimate_tokens("", &[]), 0);
        assert_eq!(estimate_tokens("abcd", &[]), 1);
        assert_eq!(estimate_tokens("", &["abcdefgh".into()]), 2);
    }

    #[test]
    fn parse_ac_state_recognises_explicit_prefixes() {
        assert_eq!(parse_ac_state("[x] done", false), (true, "done".into()));
        assert_eq!(parse_ac_state("[X] done", false), (true, "done".into()));
        assert_eq!(parse_ac_state("[ ] open", true), (false, "open".into()));
    }

    #[test]
    fn parse_ac_state_falls_back_to_terminal_default() {
        assert_eq!(
            parse_ac_state("legacy line", true),
            (true, "legacy line".into())
        );
        assert_eq!(
            parse_ac_state("legacy line", false),
            (false, "legacy line".into())
        );
    }

    #[test]
    fn write_ac_state_round_trips_through_parse() {
        let written = write_ac_state("ship it", true);
        assert_eq!(parse_ac_state(&written, false), (true, "ship it".into()));
        let unchecked = write_ac_state("ship it", false);
        assert_eq!(parse_ac_state(&unchecked, true), (false, "ship it".into()));
    }

    fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
        let mut out = String::new();
        'rows: for y in 0..buf.area.height {
            'cols: for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
                continue 'cols;
            }
            out.push('\n');
            continue 'rows;
        }
        out
    }

    fn task_with_ac(id: &str, status: &str, ac: Vec<&str>) -> tc_core::task::Task {
        let mut t = dummy_task(id, "alpha", status);
        t.acceptance_criteria = ac.into_iter().map(String::from).collect();
        t
    }

    fn render_detail_only(app: &crate::app::App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| {
                let area = f.area();
                render(app, f, area);
            })
            .expect("draw");
        buffer_text(terminal.backend().buffer())
    }

    #[test]
    fn detail_snapshot_todo_renders_unchecked_acs() {
        let task = task_with_ac("T-100", "todo", vec!["build the thing", "test the thing"]);
        let mut app = app_with(vec![task]);
        app.focus = FocusPanel::Detail;
        let text = render_detail_only(&app, 60, 24);
        assert!(text.contains("[ ] build the thing"), "{text}");
        assert!(text.contains("[ ] test the thing"), "{text}");
        // Cursor highlight on the first row when Detail is focused.
        assert!(
            text.contains("> [ ] build the thing"),
            "expected cursor on first AC: {text}"
        );
        assert!(
            text.contains("(0/2)"),
            "header count should reflect zero checked: {text}"
        );
    }

    #[test]
    fn detail_snapshot_running_shows_live_tail_block() {
        let task = task_with_ac("T-101", "in_progress", vec!["implement", "verify"]);
        let mut app = app_with(vec![task.clone()]);
        // Pretend a worker is alive for this task and the log_view has
        // pulled a few recent lines off disk.
        app.workers.push(WorkerState {
            task_id: task.id.0.clone(),
            pid: 4242,
            started_at: chrono::Utc::now(),
            worktree_path: "/tmp/wt".into(),
            status: WorkerStatus::Running,
            log_path: "/tmp/wt/log".into(),
            tmux_session: None,
        });
        app.log_view.task = Some(TaskId(task.id.0.clone()));
        app.log_view.set_lines(vec![
            "starting executor".into(),
            "step 1 ok".into(),
            "step 2 running".into(),
        ]);
        app.focus = FocusPanel::Detail;
        let text = render_detail_only(&app, 60, 24);
        assert!(text.contains("[ ] implement"), "{text}");
        assert!(
            text.contains("Live worker output"),
            "live tail block missing: {text}"
        );
        assert!(
            text.contains("[running]"),
            "worker status badge missing: {text}"
        );
        assert!(text.contains("step 2 running"), "tail line missing: {text}");
    }

    #[test]
    fn detail_snapshot_done_renders_checked_acs() {
        let task = task_with_ac("T-102", "done", vec!["ship feature", "write docs"]);
        let app = app_with(vec![task]);
        let text = render_detail_only(&app, 60, 24);
        assert!(text.contains("[x] ship feature"), "{text}");
        assert!(text.contains("[x] write docs"), "{text}");
        assert!(
            text.contains("(2/2)"),
            "header count should match all checked: {text}"
        );
        // No live tail block: this task has no active worker.
        assert!(!text.contains("Live worker output"), "{text}");
    }

    #[test]
    fn detail_snapshot_done_respects_explicit_unchecked_marker() {
        // An explicit `[ ]` prefix wins over the terminal-status default
        // -- the user can intentionally re-open a single AC after the
        // task as a whole was marked done.
        let task = task_with_ac("T-103", "done", vec!["[x] shipped", "[ ] still TODO"]);
        let app = app_with(vec![task]);
        let text = render_detail_only(&app, 60, 24);
        assert!(text.contains("[x] shipped"), "{text}");
        assert!(text.contains("[ ] still TODO"), "{text}");
        assert!(text.contains("(1/2)"), "{text}");
    }

    #[test]
    fn detail_snapshot_inline_dag_preview_lists_deps_and_blocks() {
        let mut a = dummy_task("T-200", "alpha", "todo");
        a.acceptance_criteria = vec![];
        let mut b = dummy_task("T-201", "alpha", "todo");
        b.depends_on = vec![TaskId("T-200".into())];
        let mut c = dummy_task("T-202", "alpha", "in_progress");
        c.depends_on = vec![TaskId("T-201".into())];
        let mut app = app_with(vec![a, b, c]);
        app.selected_task = 1; // T-201
        let text = render_detail_only(&app, 60, 24);
        assert!(text.contains("depends on:"), "{text}");
        assert!(text.contains("T-200"), "{text}");
        assert!(text.contains("blocks:"), "{text}");
        assert!(text.contains("T-202"), "{text}");
    }
}
