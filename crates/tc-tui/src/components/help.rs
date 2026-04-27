use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::util::centered_rect_pct;

const HELP_LINES: &[(&str, &str)] = &[
    ("j/k, Up/Down", "Navigate list"),
    ("Tab", "Cycle focus across panels"),
    ("C-w h/j/k/l", "Move focus (vim-style)"),
    ("Space", "Leader menu (which-key)"),
    ("SPC w h/j/k/l", "Move focus via leader"),
    ("SPC t ...", "Task actions (d/x/i/s/K/r/R/m)"),
    ("SPC v ...", "View toggles (g/l/?, 1-4 smart views)"),
    ("SPC / | f", "Fuzzy find (also via leader)"),
    ("SPC a A T q", "Add (form) / Quick add / Cycle theme / Quit"),
    ("Enter", "Open task card (when Tasks focused)"),
    (
        "1 / 2 / 3 / 4",
        "Smart view: Today / Upcoming / Inbox / All",
    ),
    ("/", "Fuzzy find tasks (id+title+tags, live)"),
    ("a", "Add task (full form with all fields)"),
    ("A", "Quick add task (title only)"),
    ("e", "Edit task (fullscreen form)"),
    ("d", "Mark task done"),
    ("x", "Delete task"),
    ("i", "Start working (interactive)"),
    ("y/s", "Run in background"),
    ("K", "Stop task"),
    ("r", "Review changes"),
    ("R", "Reject with feedback"),
    ("m", "Merge branch"),
    ("g", "Toggle dependencies"),
    ("l", "Toggle log view"),
    ("?", "Toggle this help"),
    (", / SPC S", "Open settings (executor, mode, sandbox)"),
    ("q / Esc", "Quit"),
    ("--- Log pager (when Log is focused) ---", ""),
    ("j / k", "Scroll one line"),
    ("PgDn / PgUp", "Scroll one page"),
    ("C-d / C-u", "Scroll half page"),
    ("g / G", "Jump to top / bottom (G re-enables follow)"),
    ("F", "Toggle tail-follow mode"),
    ("/", "Search log (live)"),
    ("n / N", "Next / previous match"),
    ("--- Input editor (Add / Reject / Filter / Search) ---", ""),
    ("Enter", "Submit"),
    ("Shift+Enter", "Newline (multi-line modes)"),
    ("trailing \\ + Enter", "Also inserts a newline"),
    ("C-a / Home", "Move to line start"),
    ("C-e / End", "Move to line end"),
    ("C-b / Left", "Move char left"),
    ("C-f / Right", "Move char right"),
    ("M-b / C-Left", "Move word left"),
    ("M-f / C-Right", "Move word right"),
    ("C-p / Up", "Line up (multi-line)"),
    ("C-n / Down", "Line down (multi-line)"),
    ("C-h / Backspace", "Delete char left"),
    ("C-d / Delete", "Delete char right"),
    ("C-w / M-Backspace", "Delete word left"),
    ("C-k / C-u", "Kill to end / to start of line"),
    ("C-v", "Paste clipboard (text or image)"),
    ("Esc", "Cancel and leave input mode"),
];

pub fn render(frame: &mut Frame<'_>) {
    let area = centered_rect_pct(50, 70, frame.area());

    let title = Line::from(vec![Span::styled(
        " Keybindings ",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]);

    let lines: Vec<Line> = HELP_LINES
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:<16}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(*desc),
            ])
        })
        .collect();

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines).block(block);

    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}
