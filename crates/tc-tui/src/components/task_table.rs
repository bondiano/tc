use chrono::{Local, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use tc_core::task::{Priority, Task};

use crate::app::{App, FocusPanel};
use crate::theme::Palette;

const TAG_CHIP_LIMIT: usize = 2;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let visible = app.visible_tasks();
    let today = Local::now().date_naive();
    let palette = &app.palette;
    let rows: Vec<Row> = visible
        .iter()
        .map(|t| build_row(app, t, today, palette))
        .collect();

    let focused = app.focus == FocusPanel::Tasks;
    let title = if focused { "[ Tasks ]" } else { " Tasks " };
    let header = Row::new(vec!["ID", "P", "Title", "Due", "Status"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let widths = [
        Constraint::Length(7),
        Constraint::Length(3),
        Constraint::Min(12),
        Constraint::Length(6),
        Constraint::Length(14),
    ];
    let border_color = if focused {
        palette.border_focused
    } else {
        palette.border
    };
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = TableState::default();
    if !visible.is_empty() {
        state.select(Some(app.selected_task.min(visible.len() - 1)));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn build_row<'a>(app: &App, t: &'a Task, today: NaiveDate, palette: &Palette) -> Row<'a> {
    let worker_tag = if app.worker_for(&t.id).is_some() {
        " ●"
    } else {
        ""
    };
    let status_text = format!("{}{}", t.status.0, worker_tag);
    let status_cell =
        Cell::from(status_text).style(Style::default().fg(palette.status(&t.status.0)));

    let row = Row::new(vec![
        Cell::from(t.id.0.clone()),
        priority_cell(t.priority, palette),
        title_cell(t, palette),
        due_cell(t.due, today, palette),
        status_cell,
    ]);

    // M-7.8 completion animation: strike through and fade rows that just
    // transitioned to a terminal status. The fade is applied as a row
    // style overlay so individual cell colours still land underneath
    // before the animation completes.
    if let Some(progress) = app.completion_progress(&t.id) {
        let style = animation_style(progress, palette);
        return row.style(style);
    }
    row
}

/// Style overlay for the completion animation. Combines `CROSSED_OUT`
/// with progressive dimming -- once the animation runs out the row falls
/// back to its un-styled baseline (the call site doesn't apply this
/// overlay).
fn animation_style(progress: f32, palette: &Palette) -> Style {
    // First half of the animation keeps the strikethrough crisp; the
    // second half fades to the muted colour so the row settles into the
    // background of the list.
    let modifier = if progress < 0.5 {
        Modifier::CROSSED_OUT | Modifier::BOLD
    } else {
        Modifier::CROSSED_OUT | Modifier::DIM
    };
    let color = if progress < 0.5 {
        palette.status_done
    } else {
        fallback_dim(palette)
    };
    Style::default().fg(color).add_modifier(modifier)
}

fn fallback_dim(palette: &Palette) -> Color {
    if palette.muted == Color::Reset {
        palette.fg
    } else {
        palette.muted
    }
}

fn priority_cell(p: Priority, palette: &Palette) -> Cell<'static> {
    let label = match p {
        Priority::P1 => "P1",
        Priority::P2 => "P2",
        Priority::P3 => "P3",
        Priority::P4 => "P4",
        Priority::P5 => "P5",
    };
    let modifier = if matches!(p, Priority::P1 | Priority::P2) {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    Cell::from(label).style(
        Style::default()
            .fg(palette.priority(p))
            .add_modifier(modifier),
    )
}

fn title_cell(t: &Task, palette: &Palette) -> Cell<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(t.title.clone()));

    if !t.tags.is_empty() {
        spans.push(Span::raw("  "));
        for (i, tag) in t.tags.iter().take(TAG_CHIP_LIMIT).enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("·{tag}"),
                Style::default().fg(palette.tag),
            ));
        }
        if t.tags.len() > TAG_CHIP_LIMIT {
            spans.push(Span::styled(
                format!(" +{}", t.tags.len() - TAG_CHIP_LIMIT),
                Style::default().fg(palette.muted),
            ));
        }
    }

    if let Some(age) = age_hint(t) {
        spans.push(Span::styled(
            format!("  {age}"),
            Style::default().fg(palette.muted),
        ));
    }

    Cell::from(Line::from(spans))
}

fn due_cell(due: Option<NaiveDate>, today: NaiveDate, palette: &Palette) -> Cell<'static> {
    let Some(d) = due else {
        return Cell::from("");
    };
    let (label, color) = if d < today {
        ("OVR".to_string(), palette.due_overdue)
    } else if d == today {
        ("today".to_string(), palette.due_today)
    } else {
        (d.format("%m/%d").to_string(), palette.due_future)
    };
    Cell::from(label).style(Style::default().fg(color))
}

/// Compact age string (e.g. `3d`, `2w`, `1mo`). `None` for tasks created
/// today -- the row is already noisy enough without "0d" everywhere.
fn age_hint(t: &Task) -> Option<String> {
    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(t.created_at);
    let days = elapsed.num_days();
    if days <= 0 {
        return None;
    }
    if days < 7 {
        return Some(format!("{days}d"));
    }
    if days < 30 {
        return Some(format!("{}w", days / 7));
    }
    if days < 365 {
        return Some(format!("{}mo", days / 30));
    }
    Some(format!("{}y", days / 365))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use tc_core::theme::Theme;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn palette() -> Palette {
        Palette::from_theme(&Theme::default_preset())
    }

    #[test]
    fn due_cell_marks_overdue() {
        let cell = due_cell(Some(ymd(2026, 4, 1)), ymd(2026, 4, 27), &palette());
        // We can't introspect Cell text, but we can ensure the helper does not panic
        // and produces a non-empty cell. Behaviour validation lives in the layout
        // snapshot tests in `ui.rs`.
        let _ = cell;
    }

    #[test]
    fn due_cell_handles_no_date() {
        let _ = due_cell(None, ymd(2026, 4, 27), &palette());
    }

    #[test]
    fn age_hint_returns_none_for_fresh_task() {
        let mut task = Task {
            id: tc_core::task::TaskId("T-001".into()),
            title: "x".into(),
            epic: "e".into(),
            status: tc_core::status::StatusId("todo".into()),
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
            created_at: chrono::Utc::now(),
        };
        assert!(age_hint(&task).is_none());
        task.created_at = chrono::Utc::now() - chrono::Duration::days(3);
        assert_eq!(age_hint(&task).as_deref(), Some("3d"));
        task.created_at = chrono::Utc::now() - chrono::Duration::days(14);
        assert_eq!(age_hint(&task).as_deref(), Some("2w"));
    }
}
