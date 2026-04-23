use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::App;
use crate::components::util::centered_rect_pct;

pub fn render(app: &App, frame: &mut Frame<'_>) {
    let Some(task) = app.selected_task() else {
        return;
    };

    let area = centered_rect_pct(75, 80, frame.area());

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

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("ID:      ", Style::default().fg(Color::Cyan)),
            Span::raw(task.id.0.clone()),
        ]),
        Line::from(vec![
            Span::styled("Status:  ", Style::default().fg(Color::Cyan)),
            Span::raw(task.status.0.clone()),
        ]),
        Line::from(vec![
            Span::styled("Epic:    ", Style::default().fg(Color::Cyan)),
            Span::raw(task.epic.clone()),
        ]),
        Line::from(vec![
            Span::styled("Depends: ", Style::default().fg(Color::Cyan)),
            Span::raw(deps_str),
        ]),
        Line::from(vec![
            Span::styled("Files:   ", Style::default().fg(Color::Cyan)),
            Span::raw(files_str),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Title",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(task.title.clone()),
    ];

    if !task.notes.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Notes",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]));
        for note_line in task.notes.lines() {
            lines.push(Line::from(note_line.to_string()));
        }
    }

    if !task.acceptance_criteria.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Acceptance Criteria",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]));
        for criterion in &task.acceptance_criteria {
            lines.push(Line::from(format!("  - {criterion}")));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  j/k scroll · Esc/q/Enter close  ",
        Style::default().fg(Color::DarkGray),
    )]));

    let title = Line::from(vec![Span::styled(
        format!(" Task: {} ", task.id.0),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((app.task_card_scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}
