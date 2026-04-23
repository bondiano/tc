use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::app::{App, FocusPanel};

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let visible = app.visible_tasks();
    let rows: Vec<Row> = visible
        .iter()
        .map(|t| {
            let worker_tag = match app.worker_for(&t.id) {
                Some(_) => " ●",
                None => "",
            };
            let status_text = format!("{}{}", t.status.0, worker_tag);
            let status_cell = Cell::from(status_text).style(status_style(&t.status.0));
            Row::new(vec![
                Cell::from(t.id.0.clone()),
                Cell::from(t.title.clone()),
                status_cell,
            ])
        })
        .collect();

    let focused = app.focus == FocusPanel::Tasks;
    let title = if focused { "[ Tasks ]" } else { " Tasks " };
    let header = Row::new(vec!["ID", "Title", "Status"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let widths = [
        Constraint::Length(8),
        Constraint::Min(10),
        Constraint::Length(18),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    let mut state = TableState::default();
    if !visible.is_empty() {
        state.select(Some(app.selected_task.min(visible.len() - 1)));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn status_style(status: &str) -> Style {
    match status {
        "todo" => Style::default().fg(Color::Gray),
        "in_progress" => Style::default().fg(Color::Yellow),
        "done" => Style::default().fg(Color::Green),
        "blocked" => Style::default().fg(Color::Red),
        "review" => Style::default().fg(Color::Cyan),
        _ => Style::default(),
    }
}
