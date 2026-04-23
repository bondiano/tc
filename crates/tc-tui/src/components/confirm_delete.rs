use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use tc_core::task::Task;

use super::util::centered_rect_fixed;

pub fn render(task: &Task, confirm_yes: bool, frame: &mut Frame<'_>) {
    let area = centered_rect_fixed(50, 7, frame.area());

    let title = Line::from(vec![Span::styled(
        " Delete Task ",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )]);

    let truncated_title = if task.title.len() > 40 {
        format!("{}...", &task.title[..40])
    } else {
        task.title.clone()
    };

    let (yes_style, no_style) = if confirm_yes {
        (
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            Style::default().fg(Color::DarkGray),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    };

    let lines = vec![
        Line::from(vec![
            Span::raw("  Delete "),
            Span::styled(
                format!("{} ", task.id.0),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(truncated_title),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(" Yes ", yes_style),
            Span::raw("   "),
            Span::styled(" No ", no_style),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "  ←/-> or h/l to switch · Enter to confirm · y/n shortcut · Esc cancel",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
