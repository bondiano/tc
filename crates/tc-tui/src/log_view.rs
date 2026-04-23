use std::cell::Cell;

use tc_core::task::TaskId;

pub const MAX_LOG_LINES: usize = 10_000;

#[derive(Debug, Default)]
pub struct LogView {
    pub lines: Vec<String>,
    pub offset: usize,
    pub follow: bool,
    pub search: String,
    pub matches: Vec<usize>,
    pub match_cursor: Option<usize>,
    pub viewport_height: Cell<u16>,
    pub task: Option<TaskId>,
}

impl LogView {
    pub fn new() -> Self {
        Self {
            follow: true,
            ..Self::default()
        }
    }

    pub fn reset(&mut self) {
        self.lines.clear();
        self.offset = 0;
        self.follow = true;
        self.search.clear();
        self.matches.clear();
        self.match_cursor = None;
    }

    /// Replace the backing lines (from a tail read). Preserves offset unless
    /// follow is on (then scroll to bottom). Recomputes search matches. Each
    /// line is sanitized to remove ANSI/OSC escape sequences so raw tmux
    /// pane captures render cleanly in the pager.
    pub fn set_lines(&mut self, new_lines: Vec<String>) {
        self.lines = new_lines.into_iter().map(|l| strip_ansi(&l)).collect();
        self.recompute_matches();
        self.clamp_offset();
        if self.follow {
            self.scroll_to_bottom_raw();
        }
    }

    /// Called on every tail read to detect task switch; resets state when the
    /// tracked task changes.
    pub fn sync_task(&mut self, task: Option<&TaskId>) {
        let changed = match (&self.task, task) {
            (None, None) => false,
            (Some(a), Some(b)) => a != b,
            _ => true,
        };
        if changed {
            self.reset();
            self.task = task.cloned();
        }
    }

    fn bottom_offset(&self) -> usize {
        let vh = self.viewport_height.get() as usize;
        self.lines.len().saturating_sub(vh)
    }

    fn clamp_offset(&mut self) {
        let max = self.bottom_offset();
        if self.offset > max {
            self.offset = max;
        }
    }

    fn scroll_to_bottom_raw(&mut self) {
        self.offset = self.bottom_offset();
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.follow = false;
        self.offset = self.offset.saturating_add(n).min(self.bottom_offset());
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.follow = false;
        self.offset = self.offset.saturating_sub(n);
    }

    pub fn scroll_to_top(&mut self) {
        self.follow = false;
        self.offset = 0;
    }

    pub fn goto_bottom(&mut self) {
        self.follow = true;
        self.scroll_to_bottom_raw();
    }

    pub fn page(&self) -> usize {
        (self.viewport_height.get() as usize)
            .saturating_sub(1)
            .max(1)
    }

    pub fn half_page(&self) -> usize {
        (self.page() / 2).max(1)
    }

    pub fn toggle_follow(&mut self) {
        self.follow = !self.follow;
        if self.follow {
            self.scroll_to_bottom_raw();
        }
    }

    pub fn set_search(&mut self, pattern: String) {
        self.search = pattern;
        self.recompute_matches();
        if self.matches.is_empty() {
            self.match_cursor = None;
        } else {
            self.match_cursor = Some(0);
            self.center_on_current_match();
        }
    }

    pub fn clear_search(&mut self) {
        self.search.clear();
        self.matches.clear();
        self.match_cursor = None;
    }

    pub fn next_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        let next = match self.match_cursor {
            Some(i) => (i + 1) % self.matches.len(),
            None => 0,
        };
        self.match_cursor = Some(next);
        self.center_on_current_match();
    }

    pub fn prev_match(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        let prev = match self.match_cursor {
            Some(i) if i > 0 => i - 1,
            _ => self.matches.len() - 1,
        };
        self.match_cursor = Some(prev);
        self.center_on_current_match();
    }

    fn recompute_matches(&mut self) {
        if self.search.is_empty() {
            self.matches.clear();
            self.match_cursor = None;
            return;
        }
        let needle = self.search.to_lowercase();
        self.matches = self
            .lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                if l.to_lowercase().contains(&needle) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();
        if let Some(c) = self.match_cursor
            && c >= self.matches.len()
        {
            self.match_cursor = if self.matches.is_empty() {
                None
            } else {
                Some(0)
            };
        }
    }

    fn center_on_current_match(&mut self) {
        let Some(c) = self.match_cursor else {
            return;
        };
        let Some(&line) = self.matches.get(c) else {
            return;
        };
        self.follow = false;
        let vh = self.viewport_height.get() as usize;
        let half = vh / 2;
        self.offset = line.saturating_sub(half).min(self.bottom_offset());
    }

    pub fn current_match_line(&self) -> Option<usize> {
        self.match_cursor.and_then(|i| self.matches.get(i).copied())
    }
}

/// Strip ANSI/OSC escape sequences and unprintable C0 bytes from a log line.
///
/// tmux's `pipe-pane` writes raw pane output including cursor moves, color
/// codes and title-setting sequences. Displaying these verbatim in the pager
/// produces ugly noise (`]0;T-017: working`, `[?1049h`, etc.), so we keep
/// just printable text.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    'scan: while let Some(c) = chars.next() {
        if c != '\x1b' {
            let code = c as u32;
            let keep = code >= 0x20 || matches!(c, '\t' | '\n' | '\r');
            if keep && code != 0x7f {
                out.push(c);
            }
            continue 'scan;
        }
        let Some(&next) = chars.peek() else {
            break 'scan;
        };
        chars.next();
        match next {
            '[' => 'csi: loop {
                let Some(&c) = chars.peek() else {
                    break 'csi;
                };
                chars.next();
                let b = c as u32;
                if (0x40..=0x7e).contains(&b) {
                    break 'csi;
                }
            },
            ']' => 'osc: loop {
                let Some(&c) = chars.peek() else {
                    break 'osc;
                };
                chars.next();
                if c == '\x07' {
                    break 'osc;
                }
                if c == '\x1b' {
                    // ST: ESC \  -- consume the trailing byte if present.
                    if matches!(chars.peek(), Some('\\')) {
                        chars.next();
                    }
                    break 'osc;
                }
            },
            '(' | ')' | '*' | '+' => {
                // Character-set designators: consume one more byte.
                chars.next();
            }
            _ => {
                // Lone single-char ESC sequence (e.g. ESC =, ESC >). Already
                // consumed `next`; nothing else to skip.
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> LogView {
        let mut v = LogView::new();
        v.viewport_height.set(5);
        v.set_lines((0..20).map(|i| format!("line {i}")).collect());
        v
    }

    #[test]
    fn new_follow_scrolls_to_bottom() {
        let v = sample();
        assert_eq!(v.offset, 20 - 5);
        assert!(v.follow);
    }

    #[test]
    fn scroll_up_disables_follow() {
        let mut v = sample();
        v.scroll_up(3);
        assert_eq!(v.offset, 20 - 5 - 3);
        assert!(!v.follow);
    }

    #[test]
    fn scroll_down_clamps_to_bottom() {
        let mut v = sample();
        v.scroll_up(10);
        v.scroll_down(50);
        assert_eq!(v.offset, 15);
    }

    #[test]
    fn goto_bottom_reenables_follow() {
        let mut v = sample();
        v.scroll_up(5);
        assert!(!v.follow);
        v.goto_bottom();
        assert!(v.follow);
        assert_eq!(v.offset, 15);
    }

    #[test]
    fn new_lines_preserve_offset_when_not_following() {
        let mut v = sample();
        v.scroll_up(3);
        let off = v.offset;
        v.set_lines((0..25).map(|i| format!("line {i}")).collect());
        assert_eq!(v.offset, off);
    }

    #[test]
    fn new_lines_follow_scrolls_to_new_bottom() {
        let mut v = sample();
        assert!(v.follow);
        v.set_lines((0..30).map(|i| format!("line {i}")).collect());
        assert_eq!(v.offset, 25);
    }

    #[test]
    fn search_finds_matches_and_centers() {
        let mut v = sample();
        v.set_search("line 7".into());
        assert_eq!(v.matches, vec![7]);
        assert_eq!(v.match_cursor, Some(0));
        assert_eq!(v.current_match_line(), Some(7));
    }

    #[test]
    fn next_match_wraps() {
        let mut v = sample();
        v.set_search("line 1".into());
        // Matches: 1, 10..=19 (11 entries)
        assert_eq!(v.match_cursor, Some(0));
        v.next_match();
        assert_eq!(v.match_cursor, Some(1));
        for _ in 0..10 {
            v.next_match();
        }
        assert_eq!(v.match_cursor, Some(0));
    }

    #[test]
    fn prev_match_wraps() {
        let mut v = sample();
        v.set_search("line 1".into());
        v.prev_match();
        assert_eq!(v.match_cursor, Some(10));
    }

    #[test]
    fn sync_task_resets_on_change() {
        let mut v = sample();
        v.scroll_up(3);
        v.set_search("line".into());
        v.sync_task(Some(&TaskId("T-001".into())));
        assert!(v.follow);
        assert!(v.search.is_empty());
        assert!(v.matches.is_empty());
    }

    #[test]
    fn strip_ansi_removes_csi_color_codes() {
        let s = "\x1b[31mred\x1b[0m plain";
        assert_eq!(strip_ansi(s), "red plain");
    }

    #[test]
    fn strip_ansi_removes_osc_title() {
        let s = "before\x1b]0;window title\x07after";
        assert_eq!(strip_ansi(s), "beforeafter");
    }

    #[test]
    fn strip_ansi_removes_osc_terminated_by_st() {
        let s = "x\x1b]0;title\x1b\\y";
        assert_eq!(strip_ansi(s), "xy");
    }

    #[test]
    fn strip_ansi_removes_alt_screen_toggle() {
        let s = "\x1b[?1049h\x1b[?1049lhello";
        assert_eq!(strip_ansi(s), "hello");
    }

    #[test]
    fn strip_ansi_drops_backspace_and_bel() {
        let s = "ab\x08c\x07d";
        assert_eq!(strip_ansi(s), "abcd");
    }

    #[test]
    fn strip_ansi_preserves_tabs_and_newlines() {
        let s = "a\tb\nc";
        assert_eq!(strip_ansi(s), "a\tb\nc");
    }

    #[test]
    fn set_lines_sanitizes_incoming_lines() {
        let mut v = LogView::new();
        v.viewport_height.set(3);
        v.set_lines(vec![
            "\x1b[1mbold\x1b[0m".into(),
            "\x1b]0;title\x07plain".into(),
        ]);
        assert_eq!(v.lines, vec!["bold", "plain"]);
    }

    #[test]
    fn sync_task_preserves_state_when_same() {
        let mut v = LogView::new();
        v.viewport_height.set(5);
        v.task = Some(TaskId("T-001".into()));
        v.set_lines((0..20).map(|i| format!("line {i}")).collect());
        v.scroll_up(4);
        let off = v.offset;
        v.sync_task(Some(&TaskId("T-001".into())));
        assert_eq!(v.offset, off);
    }
}
