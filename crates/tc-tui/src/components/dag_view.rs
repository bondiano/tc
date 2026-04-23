use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, FocusPanel};

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let focused = app.focus == FocusPanel::Dag;
    let title = if focused {
        "[ Dependencies ]"
    } else {
        " Dependencies "
    };
    let block = Block::default().borders(Borders::ALL).title(title);

    let Some(task) = app.selected_task() else {
        frame.render_widget(Paragraph::new("(none)").block(block), area);
        return;
    };

    let deps = app.dag.dependencies(&task.id);
    let dependents = app.dag.dependents(&task.id);

    let mut lines = Vec::new();
    lines.push(Line::from("Depends on:"));
    if deps.is_empty() {
        lines.push(Line::from("  (none)"));
    } else {
        'deps: for d in &deps {
            lines.push(Line::from(format!("  {}", d.0)));
            continue 'deps;
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Blocks:"));
    if dependents.is_empty() {
        lines.push(Line::from("  (none)"));
    } else {
        'dependents: for d in &dependents {
            lines.push(Line::from(format!("  {}", d.0)));
            continue 'dependents;
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}
