use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use tc_executor::sandbox::{
    SandboxProvider, detect_bwrap, detect_nono, detect_sandbox_exec, detect_sbx,
};

use crate::app::{App, SettingsField, SettingsState};

use super::util::centered_rect_pct;

pub const EXECUTOR_OPTIONS: &[&str] = &["claude", "opencode", "codex", "pi", "gemini", "all"];
pub const MODE_OPTIONS: &[&str] = &["accept", "interactive"];
pub const SANDBOX_OPTIONS: &[&str] = &["auto", "never", "always"];

pub fn render(app: &App, frame: &mut Frame<'_>) {
    let Some(state) = &app.settings else {
        return;
    };

    let area = centered_rect_pct(60, 80, frame.area());

    let block = Block::default()
        .title(Line::from(Span::styled(
            " Settings ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let inner = inset(area, 1, 1);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // executor
            Constraint::Length(3), // mode
            Constraint::Length(3), // sandbox
            Constraint::Min(6),    // info
            Constraint::Length(2), // footer
        ])
        .split(inner);

    render_field(
        frame,
        rows[0],
        "Executor",
        &format_choice_row(EXECUTOR_OPTIONS, &state.executor),
        state.field == SettingsField::Executor,
    );
    render_field(
        frame,
        rows[1],
        "Mode",
        &format_choice_row(MODE_OPTIONS, &state.mode),
        state.field == SettingsField::Mode,
    );
    render_field(
        frame,
        rows[2],
        "Sandbox policy",
        &format_choice_row(SANDBOX_OPTIONS, &state.sandbox),
        state.field == SettingsField::Sandbox,
    );

    render_info(frame, rows[3], state, app);
    render_footer(frame, rows[4], state);
}

fn render_field(frame: &mut Frame<'_>, area: Rect, label: &str, value: &str, active: bool) {
    let border_color = if active { Color::Cyan } else { Color::DarkGray };
    let label_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let line = Line::from(vec![
        Span::styled(format!(" {label}: "), label_style),
        Span::raw(value),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let p = Paragraph::new(line).block(block);
    frame.render_widget(p, area);
}

fn format_choice_row(options: &[&str], current: &str) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(options.len());
    for opt in options {
        if *opt == current {
            parts.push(format!("[{opt}]"));
        } else {
            parts.push((*opt).to_string());
        }
    }
    parts.join("  ")
}

fn render_info(frame: &mut Frame<'_>, area: Rect, state: &SettingsState, _app: &App) {
    let sbx = detect_sbx();
    let nono = detect_nono();
    let sx = detect_sandbox_exec();
    let bwrap = detect_bwrap();

    let provider_name = match (state.sandbox.as_str(), sbx, nono, sx, bwrap) {
        ("never", _, _, _, _) => "none (disabled)",
        (_, true, _, _, _) => SandboxProvider::Sbx.name(),
        (_, _, true, _, _) => SandboxProvider::Nono.name(),
        (_, _, _, true, _) => SandboxProvider::SandboxExec.name(),
        (_, _, _, _, true) => SandboxProvider::Bwrap.name(),
        _ => "none",
    };

    let lines: Vec<Line> = vec![
        Line::from(vec![Span::styled(
            " Sandbox installation ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::raw("  active provider: "),
            Span::styled(
                provider_name.to_string(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        install_line("sbx", "Docker AI Sandboxes (microVM)", sbx),
        install_line("nono", "Landlock (Linux)", nono),
        install_line("sandbox-exec", "macOS seatbelt", sx),
        install_line("bwrap", "bubblewrap (Linux)", bwrap),
    ];

    let p = Paragraph::new(lines);
    frame.render_widget(p, area);
}

fn install_line(name: &str, desc: &str, present: bool) -> Line<'static> {
    let (marker, color) = if present {
        ("installed  ", Color::Green)
    } else {
        ("missing    ", Color::Red)
    };
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            marker.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{name:<14}"), Style::default().fg(Color::Cyan)),
        Span::raw(desc.to_string()),
    ])
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &SettingsState) {
    let dirty = if state.dirty { " *unsaved" } else { "" };
    let line = Line::from(vec![
        Span::styled(
            "  j/k",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" move  "),
        Span::styled(
            "h/l",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" change  "),
        Span::styled(
            "Enter",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" save  "),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" close"),
        Span::styled(
            dirty.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn inset(area: Rect, dx: u16, dy: u16) -> Rect {
    let x = area.x + dx;
    let y = area.y + dy;
    let w = area.width.saturating_sub(dx * 2);
    let h = area.height.saturating_sub(dy * 2);
    Rect::new(x, y, w, h)
}
