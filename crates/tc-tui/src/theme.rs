//! TUI-side bridge between `tc_core::theme::Theme` and ratatui colors.
//!
//! Components should pull `Color` values from `App::theme_palette()` rather
//! than hard-coding `Color::Yellow` etc. so theme switches take effect
//! globally (M-7.5).

use ratatui::style::Color;
use tc_core::theme::{NamedColor, Theme, ThemeColor};

#[derive(Debug, Clone)]
pub struct Palette {
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub border: Color,
    pub border_focused: Color,
    pub highlight_fg: Color,
    pub highlight_bg: Color,

    pub tab_active_fg: Color,
    pub tab_active_bg: Color,
    pub tab_inactive: Color,
    pub tag: Color,

    pub priority_p1: Color,
    pub priority_p2: Color,
    pub priority_p3: Color,
    pub priority_p4: Color,
    pub priority_p5: Color,

    pub status_todo: Color,
    pub status_in_progress: Color,
    pub status_review: Color,
    pub status_done: Color,
    pub status_blocked: Color,

    pub due_overdue: Color,
    pub due_today: Color,
    pub due_future: Color,
}

impl Palette {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            fg: to_color(theme.fg),
            muted: to_color(theme.muted),
            accent: to_color(theme.accent),
            border: to_color(theme.border),
            border_focused: to_color(theme.border_focused),
            highlight_fg: to_color(theme.highlight_fg),
            highlight_bg: to_color(theme.highlight_bg),
            tab_active_fg: to_color(theme.tab_active_fg),
            tab_active_bg: to_color(theme.tab_active_bg),
            tab_inactive: to_color(theme.tab_inactive),
            tag: to_color(theme.tag),
            priority_p1: to_color(theme.priority_p1),
            priority_p2: to_color(theme.priority_p2),
            priority_p3: to_color(theme.priority_p3),
            priority_p4: to_color(theme.priority_p4),
            priority_p5: to_color(theme.priority_p5),
            status_todo: to_color(theme.status_todo),
            status_in_progress: to_color(theme.status_in_progress),
            status_review: to_color(theme.status_review),
            status_done: to_color(theme.status_done),
            status_blocked: to_color(theme.status_blocked),
            due_overdue: to_color(theme.due_overdue),
            due_today: to_color(theme.due_today),
            due_future: to_color(theme.due_future),
        }
    }

    /// Look up the priority slot by enum value -- avoids match-on-Priority
    /// scattering across components.
    pub fn priority(&self, p: tc_core::task::Priority) -> Color {
        use tc_core::task::Priority;
        match p {
            Priority::P1 => self.priority_p1,
            Priority::P2 => self.priority_p2,
            Priority::P3 => self.priority_p3,
            Priority::P4 => self.priority_p4,
            Priority::P5 => self.priority_p5,
        }
    }

    pub fn status(&self, status: &str) -> Color {
        match status {
            "todo" => self.status_todo,
            "in_progress" => self.status_in_progress,
            "review" => self.status_review,
            "done" => self.status_done,
            "blocked" => self.status_blocked,
            _ => self.fg,
        }
    }
}

fn to_color(c: ThemeColor) -> Color {
    match c {
        ThemeColor::Named(NamedColor::Reset) => Color::Reset,
        ThemeColor::Named(NamedColor::Black) => Color::Black,
        ThemeColor::Named(NamedColor::DarkGray) => Color::DarkGray,
        ThemeColor::Named(NamedColor::Gray) => Color::Gray,
        ThemeColor::Named(NamedColor::White) => Color::White,
        ThemeColor::Named(NamedColor::Red) => Color::Red,
        ThemeColor::Named(NamedColor::Green) => Color::Green,
        ThemeColor::Named(NamedColor::Yellow) => Color::Yellow,
        ThemeColor::Named(NamedColor::Blue) => Color::Blue,
        ThemeColor::Named(NamedColor::Magenta) => Color::Magenta,
        ThemeColor::Named(NamedColor::Cyan) => Color::Cyan,
        ThemeColor::Named(NamedColor::LightRed) => Color::LightRed,
        ThemeColor::Named(NamedColor::LightGreen) => Color::LightGreen,
        ThemeColor::Named(NamedColor::LightYellow) => Color::LightYellow,
        ThemeColor::Named(NamedColor::LightBlue) => Color::LightBlue,
        ThemeColor::Named(NamedColor::LightMagenta) => Color::LightMagenta,
        ThemeColor::Named(NamedColor::LightCyan) => Color::LightCyan,
        ThemeColor::Rgb { r, g, b } => Color::Rgb(r, g, b),
    }
}

/// Resolve the user's configured theme name to a built-in preset, falling
/// back to the default if the name is unrecognized. Components use this so
/// an unknown theme never crashes the TUI -- validation already happens at
/// config load.
pub fn resolve(name: &str) -> Theme {
    Theme::by_name(name).unwrap_or_else(Theme::default_preset)
}
