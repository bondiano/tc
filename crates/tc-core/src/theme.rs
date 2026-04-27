//! Color palette for the TUI (M-7.5).
//!
//! tc-core has no UI dependency, so colors are kept as a small data-only
//! enum (`ThemeColor`). The TUI crate converts each entry to its
//! `ratatui::style::Color` equivalent at render time.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedColor {
    Reset,
    Black,
    DarkGray,
    Gray,
    White,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThemeColor {
    Named(NamedColor),
    Rgb { r: u8, g: u8, b: u8 },
}

impl ThemeColor {
    pub const RESET: Self = Self::Named(NamedColor::Reset);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb { r, g, b }
    }

    pub const fn named(c: NamedColor) -> Self {
        Self::Named(c)
    }
}

/// Palette consumed by the TUI. Every named slot maps to a logical role so
/// individual presets only have to override the ones that differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,

    // Text + chrome
    pub fg: ThemeColor,
    pub muted: ThemeColor,
    pub accent: ThemeColor,
    pub border: ThemeColor,
    pub border_focused: ThemeColor,
    pub highlight_fg: ThemeColor,
    pub highlight_bg: ThemeColor,

    // Tabs / chips
    pub tab_active_fg: ThemeColor,
    pub tab_active_bg: ThemeColor,
    pub tab_inactive: ThemeColor,
    pub tag: ThemeColor,

    // Priority
    pub priority_p1: ThemeColor,
    pub priority_p2: ThemeColor,
    pub priority_p3: ThemeColor,
    pub priority_p4: ThemeColor,
    pub priority_p5: ThemeColor,

    // Status
    pub status_todo: ThemeColor,
    pub status_in_progress: ThemeColor,
    pub status_review: ThemeColor,
    pub status_done: ThemeColor,
    pub status_blocked: ThemeColor,

    // Due
    pub due_overdue: ThemeColor,
    pub due_today: ThemeColor,
    pub due_future: ThemeColor,
}

impl Theme {
    /// Names of every built-in preset, in display order.
    pub const PRESET_NAMES: &'static [&'static str] =
        &["default", "dim", "high-contrast", "solarized"];

    pub fn default_preset() -> Self {
        Self {
            name: "default".into(),
            fg: ThemeColor::RESET,
            muted: ThemeColor::named(NamedColor::DarkGray),
            accent: ThemeColor::named(NamedColor::Yellow),
            border: ThemeColor::named(NamedColor::Gray),
            border_focused: ThemeColor::named(NamedColor::Yellow),
            highlight_fg: ThemeColor::RESET,
            highlight_bg: ThemeColor::RESET,
            tab_active_fg: ThemeColor::named(NamedColor::Black),
            tab_active_bg: ThemeColor::named(NamedColor::Yellow),
            tab_inactive: ThemeColor::named(NamedColor::Gray),
            tag: ThemeColor::named(NamedColor::Cyan),
            priority_p1: ThemeColor::named(NamedColor::Red),
            priority_p2: ThemeColor::named(NamedColor::Yellow),
            priority_p3: ThemeColor::RESET,
            priority_p4: ThemeColor::named(NamedColor::Gray),
            priority_p5: ThemeColor::named(NamedColor::DarkGray),
            status_todo: ThemeColor::named(NamedColor::Gray),
            status_in_progress: ThemeColor::named(NamedColor::Yellow),
            status_review: ThemeColor::named(NamedColor::Cyan),
            status_done: ThemeColor::named(NamedColor::Green),
            status_blocked: ThemeColor::named(NamedColor::Red),
            due_overdue: ThemeColor::named(NamedColor::Red),
            due_today: ThemeColor::named(NamedColor::Yellow),
            due_future: ThemeColor::named(NamedColor::Cyan),
        }
    }

    pub fn dim_preset() -> Self {
        Self {
            name: "dim".into(),
            fg: ThemeColor::named(NamedColor::Gray),
            muted: ThemeColor::named(NamedColor::DarkGray),
            accent: ThemeColor::named(NamedColor::LightCyan),
            border: ThemeColor::named(NamedColor::DarkGray),
            border_focused: ThemeColor::named(NamedColor::LightCyan),
            highlight_fg: ThemeColor::named(NamedColor::White),
            highlight_bg: ThemeColor::named(NamedColor::DarkGray),
            tab_active_fg: ThemeColor::named(NamedColor::Black),
            tab_active_bg: ThemeColor::named(NamedColor::LightCyan),
            tab_inactive: ThemeColor::named(NamedColor::DarkGray),
            tag: ThemeColor::named(NamedColor::LightBlue),
            priority_p1: ThemeColor::named(NamedColor::LightRed),
            priority_p2: ThemeColor::named(NamedColor::LightYellow),
            priority_p3: ThemeColor::named(NamedColor::Gray),
            priority_p4: ThemeColor::named(NamedColor::DarkGray),
            priority_p5: ThemeColor::named(NamedColor::DarkGray),
            status_todo: ThemeColor::named(NamedColor::Gray),
            status_in_progress: ThemeColor::named(NamedColor::LightYellow),
            status_review: ThemeColor::named(NamedColor::LightCyan),
            status_done: ThemeColor::named(NamedColor::LightGreen),
            status_blocked: ThemeColor::named(NamedColor::LightRed),
            due_overdue: ThemeColor::named(NamedColor::LightRed),
            due_today: ThemeColor::named(NamedColor::LightYellow),
            due_future: ThemeColor::named(NamedColor::LightBlue),
        }
    }

    pub fn high_contrast_preset() -> Self {
        Self {
            name: "high-contrast".into(),
            fg: ThemeColor::named(NamedColor::White),
            muted: ThemeColor::named(NamedColor::Gray),
            accent: ThemeColor::named(NamedColor::LightYellow),
            border: ThemeColor::named(NamedColor::White),
            border_focused: ThemeColor::named(NamedColor::LightYellow),
            highlight_fg: ThemeColor::named(NamedColor::Black),
            highlight_bg: ThemeColor::named(NamedColor::White),
            tab_active_fg: ThemeColor::named(NamedColor::Black),
            tab_active_bg: ThemeColor::named(NamedColor::White),
            tab_inactive: ThemeColor::named(NamedColor::Gray),
            tag: ThemeColor::named(NamedColor::LightCyan),
            priority_p1: ThemeColor::named(NamedColor::LightRed),
            priority_p2: ThemeColor::named(NamedColor::LightYellow),
            priority_p3: ThemeColor::named(NamedColor::White),
            priority_p4: ThemeColor::named(NamedColor::Gray),
            priority_p5: ThemeColor::named(NamedColor::Gray),
            status_todo: ThemeColor::named(NamedColor::White),
            status_in_progress: ThemeColor::named(NamedColor::LightYellow),
            status_review: ThemeColor::named(NamedColor::LightCyan),
            status_done: ThemeColor::named(NamedColor::LightGreen),
            status_blocked: ThemeColor::named(NamedColor::LightRed),
            due_overdue: ThemeColor::named(NamedColor::LightRed),
            due_today: ThemeColor::named(NamedColor::LightYellow),
            due_future: ThemeColor::named(NamedColor::LightCyan),
        }
    }

    pub fn solarized_preset() -> Self {
        // Solarized palette (Ethan Schoonover) -- truecolor RGB values.
        let base01 = ThemeColor::rgb(0x58, 0x6e, 0x75);
        let base1 = ThemeColor::rgb(0x93, 0xa1, 0xa1);
        let base3 = ThemeColor::rgb(0xfd, 0xf6, 0xe3);
        let yellow = ThemeColor::rgb(0xb5, 0x89, 0x00);
        let orange = ThemeColor::rgb(0xcb, 0x4b, 0x16);
        let red = ThemeColor::rgb(0xdc, 0x32, 0x2f);
        let blue = ThemeColor::rgb(0x26, 0x8b, 0xd2);
        let cyan = ThemeColor::rgb(0x2a, 0xa1, 0x98);
        let green = ThemeColor::rgb(0x85, 0x99, 0x00);

        Self {
            name: "solarized".into(),
            fg: base1,
            muted: base01,
            accent: yellow,
            border: base01,
            border_focused: yellow,
            highlight_fg: base3,
            highlight_bg: base01,
            tab_active_fg: base3,
            tab_active_bg: yellow,
            tab_inactive: base01,
            tag: cyan,
            priority_p1: red,
            priority_p2: orange,
            priority_p3: base1,
            priority_p4: base01,
            priority_p5: base01,
            status_todo: base1,
            status_in_progress: yellow,
            status_review: blue,
            status_done: green,
            status_blocked: red,
            due_overdue: red,
            due_today: orange,
            due_future: cyan,
        }
    }

    /// Resolve a preset by name. Names are matched case-insensitively, and
    /// both `high-contrast` and `high_contrast` are accepted.
    pub fn by_name(name: &str) -> Option<Self> {
        let key = name.to_lowercase().replace('_', "-");
        match key.as_str() {
            "default" => Some(Self::default_preset()),
            "dim" => Some(Self::dim_preset()),
            "high-contrast" | "hc" => Some(Self::high_contrast_preset()),
            "solarized" => Some(Self::solarized_preset()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_resolvable_by_name() {
        for n in Theme::PRESET_NAMES {
            assert!(Theme::by_name(n).is_some(), "preset {n} should resolve");
        }
    }

    #[test]
    fn high_contrast_alias() {
        assert!(Theme::by_name("high_contrast").is_some());
        assert!(Theme::by_name("high-contrast").is_some());
        assert!(Theme::by_name("HC").is_some());
    }

    #[test]
    fn unknown_returns_none() {
        assert!(Theme::by_name("nope").is_none());
    }

    #[test]
    fn presets_have_distinct_accents() {
        let accents: Vec<_> = Theme::PRESET_NAMES
            .iter()
            .map(|n| Theme::by_name(n).unwrap().accent)
            .collect();
        // At least three of the four accents should differ -- if everything
        // collapses to the same color, the presets aren't actually presets.
        let mut unique = accents.clone();
        unique.dedup();
        assert!(
            unique.len() >= 3,
            "presets share too many accent colors: {accents:?}"
        );
    }

    #[test]
    fn theme_round_trips_through_yaml() {
        let yaml = serde_yaml_ng::to_string(&Theme::solarized_preset()).unwrap();
        let back: Theme = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(back, Theme::solarized_preset());
    }
}
