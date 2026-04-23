use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use tc_core::config::{ExecutionMode, ExecutorKind, SandboxPolicy};
use tc_executor::any::is_installed;
use tc_executor::sandbox::{
    SandboxProvider, detect_bwrap, detect_nono, detect_sandbox_exec, detect_sbx,
};

use crate::app::{App, SettingsField, SettingsState};

use super::util::centered_rect_pct;

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

    render_executor_field(
        frame,
        rows[0],
        state.executor,
        state.field == SettingsField::Executor,
        app,
    );
    render_field(
        frame,
        rows[1],
        "Mode",
        format_choice_spans(ExecutionMode::ALL, state.mode),
        state.field == SettingsField::Mode,
    );
    render_field(
        frame,
        rows[2],
        "Sandbox policy",
        format_choice_spans(SandboxPolicy::ALL, state.sandbox),
        state.field == SettingsField::Sandbox,
    );

    render_info(frame, rows[3], state, app);
    render_footer(frame, rows[4], state);
}

fn render_field(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value_spans: Vec<Span<'static>>,
    active: bool,
) {
    let border_color = if active { Color::Cyan } else { Color::DarkGray };
    let label_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(value_spans.len() + 1);
    spans.push(Span::styled(format!(" {label}: "), label_style));
    spans.extend(value_spans);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let p = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(p, area);
}

fn render_executor_field(
    frame: &mut Frame<'_>,
    area: Rect,
    current: ExecutorKind,
    active: bool,
    app: &App,
) {
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(ExecutorKind::ALL.len() * 2);
    let mut first = true;
    for kind in ExecutorKind::ALL {
        if !first {
            spans.push(Span::raw("  "));
        }
        first = false;
        let installed = is_installed(*kind, &app.config);
        let is_current = *kind == current;
        let label = if is_current {
            format!("[{kind}]")
        } else {
            kind.to_string()
        };
        let mut style = Style::default();
        if !installed {
            style = style.fg(Color::DarkGray);
        } else if is_current {
            style = style.fg(Color::Cyan).add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(label, style));
    }
    render_field(frame, area, "Executor", spans, active);
}

fn format_choice_spans<T>(options: &[T], current: T) -> Vec<Span<'static>>
where
    T: std::fmt::Display + Copy + PartialEq,
{
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(options.len() * 2);
    let mut first = true;
    for opt in options {
        if !first {
            spans.push(Span::raw("  "));
        }
        first = false;
        if *opt == current {
            spans.push(Span::styled(
                format!("[{opt}]"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw(opt.to_string()));
        }
    }
    spans
}

fn render_info(frame: &mut Frame<'_>, area: Rect, state: &SettingsState, _app: &App) {
    let sbx = detect_sbx();
    let nono = detect_nono();
    let sx = detect_sandbox_exec();
    let bwrap = detect_bwrap();

    let provider_name = match (state.sandbox, sbx, nono, sx, bwrap) {
        (SandboxPolicy::Never, _, _, _, _) => "none (disabled)",
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
