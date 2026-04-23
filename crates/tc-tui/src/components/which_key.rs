use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::keybind::PendingChord;

const CTRL_W_MENU: &[(&str, &str)] = &[
    ("h", "focus left"),
    ("j", "focus below"),
    ("k", "focus above"),
    ("l", "focus right"),
    ("Esc", "cancel"),
];

const LEADER_MENU: &[(&str, &str)] = &[
    ("w", "+window"),
    ("t", "+task"),
    ("v", "+view"),
    ("/", "filter"),
    ("a", "add task"),
    ("S", "settings"),
    ("q", "quit"),
    ("Esc", "cancel"),
];

const LEADER_WINDOW_MENU: &[(&str, &str)] = &[
    ("h", "focus left"),
    ("j", "focus below"),
    ("k", "focus above"),
    ("l", "focus right"),
    ("Esc", "cancel"),
];

const LEADER_TASK_MENU: &[(&str, &str)] = &[
    ("d", "mark done"),
    ("x", "delete task"),
    ("i", "start (interactive)"),
    ("s", "run in background"),
    ("K", "stop task"),
    ("r", "review changes"),
    ("R", "reject"),
    ("m", "merge branch"),
    ("Esc", "cancel"),
];

const LEADER_VIEW_MENU: &[(&str, &str)] = &[
    ("g", "toggle dependencies"),
    ("l", "toggle log"),
    ("?", "help popup"),
    ("Esc", "cancel"),
];

pub fn render(frame: &mut Frame<'_>, chord: PendingChord) {
    let (title, entries) = match chord {
        PendingChord::CtrlW => (" Window (C-w) ", CTRL_W_MENU),
        PendingChord::Leader => (" Leader (SPC) ", LEADER_MENU),
        PendingChord::LeaderWindow => (" Window (SPC w) ", LEADER_WINDOW_MENU),
        PendingChord::LeaderTask => (" Task (SPC t) ", LEADER_TASK_MENU),
        PendingChord::LeaderView => (" View (SPC v) ", LEADER_VIEW_MENU),
        PendingChord::None => return,
    };

    let lines: Vec<Line> = entries
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:<6}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(*desc),
            ])
        })
        .collect();

    let height = (entries.len() as u16 + 2).min(12);
    let area = bottom_popup(50, height, frame.area());

    let block = Block::default()
        .title(Line::from(Span::styled(
            title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines).block(block);

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}

fn bottom_popup(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::End)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}
