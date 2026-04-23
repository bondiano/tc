use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, FocusPanel};

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.focus == FocusPanel::Detail;
    let title = if focused { "[ Detail ]" } else { " Detail " };
    let block = Block::default().borders(Borders::ALL).title(title);

    let Some(task) = app.selected_task() else {
        let p = Paragraph::new("(no task selected)").block(block);
        frame.render_widget(p, area);
        return;
    };

    let deps = app.dag.dependencies(&task.id);
    let deps_str = if deps.is_empty() {
        "(none)".to_string()
    } else {
        deps.iter()
            .map(|d| d.0.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };

    let files_str = if task.files.is_empty() {
        "(none)".to_string()
    } else {
        task.files.join(", ")
    };

    let token_estimate = estimate_tokens(&task.notes, &task.files);

    let worker_line = app
        .worker_for(&task.id)
        .map(|w| format!("Agent: {}", w.status));

    let last_output = if app.worker_for(&task.id).is_some() {
        app.log_view
            .lines
            .last()
            .filter(|l| !l.trim().is_empty())
            .map(|l| format!("Last output: {l}"))
    } else {
        None
    };

    let mut lines = vec![
        Line::from(vec![Span::raw("ID:    "), Span::raw(task.id.0.clone())]),
        Line::from(vec![Span::raw("Title: "), Span::raw(task.title.clone())]),
        Line::from(format!("Epic:  {}", task.epic)),
        Line::from(format!("Status: {}", task.status.0)),
        Line::from(format!("Depends on: {deps_str}")),
        Line::from(format!("Files: {files_str}")),
        Line::from(format!("Context size (est.): {token_estimate}")),
    ];
    if let Some(wl) = worker_line {
        lines.push(Line::from(wl));
    }
    if let Some(lo) = last_output {
        lines.push(Line::from(lo));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Notes:"));
    lines.push(Line::from(task.notes.clone()));
    let p = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn estimate_tokens(notes: &str, files: &[String]) -> usize {
    notes.len() / 4 + files.iter().map(|f| f.len() / 4).sum::<usize>()
}
