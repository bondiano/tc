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
        render_live_tail(app, frame, chunks[1]);
    }
}

fn render_metadata(app: &App, task: &Task, frame: &mut Frame<'_>, area: Rect) {
    let palette = &app.palette;
    let today = Local::now().date_naive();
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(label_value("ID", &task.id.0, palette));
    lines.push(label_value("Title", &task.title, palette));
    lines.push(label_value("Epic", &task.epic, palette));

    lines.push(Line::from(vec![
        muted_label("Status", palette),
        Span::styled(
            task.status.0.clone(),
            Style::default().fg(palette.status(&task.status.0)),
        ),
    ]));

    lines.push(Line::from(vec![
        muted_label("Priority", palette),
        Span::styled(
            task.priority.to_string().to_uppercase(),
            Style::default()
                .fg(palette.priority(task.priority))
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if !task.tags.is_empty() {
        let mut spans: Vec<Span<'static>> = vec![muted_label("Tags", palette)];
        for (i, tag) in task.tags.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("·{tag}"),
                Style::default().fg(palette.tag),
            ));
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
        for criterion in &task.acceptance_criteria {
            lines.push(ac_row(criterion, task, &app.status_machine, palette));
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
        for note_line in task.notes.lines() {
            lines.push(Line::from(note_line.to_string()));
        }
    }

    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn render_live_tail(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let palette = &app.palette;
    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        " Live worker output ",
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(worker) = app
        .selected_task()
        .as_ref()
        .and_then(|t| app.worker_for(&t.id))
    {
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

    let lines: Vec<Line<'static>> = recent_log_lines(app, LIVE_TAIL_LINES)
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

/// Pull the last `n` non-empty lines from the in-memory `LogView`. We avoid
/// re-reading from disk -- `App::tail_log` already does that on tick when
/// the log is shown -- and gracefully fall back to the empty slice when the
/// pager hasn't been opened yet.
fn recent_log_lines(app: &App, n: usize) -> Vec<String> {
    if app.log_view.lines.is_empty() {
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
        for dep in shown {
            lines.push(dep_line(dep, &by_id, palette));
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
        for dep in dependents.iter().take(INLINE_DAG_BUDGET) {
            lines.push(dep_line(dep, &by_id, palette));
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
        Span::styled(format!("[{status_label}] "), Style::default().fg(status_color)),
        Span::styled(title, Style::default().fg(palette.muted)),
    ])
}

fn ac_header(task: &Task, sm: &StatusMachine, palette: &Palette) -> Line<'static> {
    let total = task.acceptance_criteria.len();
    let completed = if sm.is_terminal(&task.status) {
        total
    } else {
        0
    };
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

fn ac_row(
    criterion: &str,
    task: &Task,
    sm: &StatusMachine,
    palette: &Palette,
) -> Line<'static> {
    let checked = sm.is_terminal(&task.status);
    let (mark, color) = if checked {
        ("[x] ", palette.status_done)
    } else {
        ("[ ] ", palette.muted)
    };
    let mut text_style = Style::default();
    if checked {
        text_style = text_style.add_modifier(Modifier::CROSSED_OUT);
    }
    Line::from(vec![
        Span::raw("  "),
        Span::styled(mark, Style::default().fg(color)),
        Span::styled(criterion.to_string(), text_style),
    ])
}

fn label_value(label: &str, value: &str, palette: &Palette) -> Line<'static> {
    Line::from(vec![muted_label(label, palette), Span::raw(value.to_string())])
}

fn muted_label(label: &str, palette: &Palette) -> Span<'static> {
    Span::styled(
        format!("{label:<10}"),
        Style::default().fg(palette.muted),
    )
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
}
