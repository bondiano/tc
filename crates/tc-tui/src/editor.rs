//! Multi-line text editor backing the TUI's input modes.
//!
//! Pure state: no I/O, no ratatui, no crossterm. Byte-indexed cursor with a
//! (row, col) model; all mutators keep `col` on a UTF-8 char boundary so
//! `lines[row].insert(col, c)` is always safe.

const DEFAULT_MAX_ROWS: u16 = 10;

#[derive(Debug, Clone)]
pub struct Editor {
    lines: Vec<String>,
    row: usize,
    col: usize,
    pub max_rows: u16,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
            max_rows: DEFAULT_MAX_ROWS,
        }
    }
}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the whole buffer and place the cursor at the end.
    pub fn set_text(&mut self, text: &str) {
        self.lines = text.split('\n').map(String::from).collect();
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.row = self.lines.len() - 1;
        self.col = self.lines[self.row].len();
    }

    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.row = 0;
        self.col = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.lines.len() == 1 && self.lines[0].is_empty()
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Full buffer as one string, lines joined with `\n`.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Same as `text()` but newlines collapsed to single spaces. Used for
    /// single-line consumers (filter, log search) that can't handle `\n`.
    pub fn text_single_line(&self) -> String {
        self.lines.join(" ")
    }

    /// Byte-based cursor inside `lines[row]`, plus row index.
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    /// Visual column (char count from start of current line to `col`). Use
    /// for mapping the byte cursor onto a terminal column. Double-width
    /// characters (CJK) still count as one; this matches what ratatui does
    /// for the surrounding text when we don't opt into unicode-width.
    pub fn visual_col(&self) -> usize {
        self.lines[self.row][..self.col].chars().count()
    }

    pub fn ends_with_backslash(&self) -> bool {
        self.lines
            .last()
            .map(|l| l.ends_with('\\'))
            .unwrap_or(false)
    }

    /// Drop a single trailing `\` from the last line. Used to strip the
    /// "line continuation" sigil before inserting a newline.
    pub fn pop_trailing_backslash(&mut self) {
        let Some(last) = self.lines.last_mut() else {
            return;
        };
        if last.pop() == Some('\\') && self.row == self.lines.len() - 1 && self.col > 0 {
            self.col -= 1;
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if c == '\n' {
            self.insert_newline();
            return;
        }
        let line = &mut self.lines[self.row];
        line.insert(self.col, c);
        self.col += c.len_utf8();
    }

    /// Paste path: splits on `\n` and inserts. Each embedded newline
    /// advances the cursor to a new row.
    pub fn insert_str(&mut self, s: &str) {
        'segments: for (i, segment) in s.split('\n').enumerate() {
            if i > 0 {
                self.insert_newline();
            }
            if segment.is_empty() {
                continue 'segments;
            }
            let line = &mut self.lines[self.row];
            line.insert_str(self.col, segment);
            self.col += segment.len();
        }
    }

    pub fn insert_newline(&mut self) {
        let tail = self.lines[self.row].split_off(self.col);
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.col = 0;
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            let prev = prev_char_boundary(&self.lines[self.row], self.col);
            self.lines[self.row].replace_range(prev..self.col, "");
            self.col = prev;
            return;
        }
        if self.row == 0 {
            return;
        }
        let current = self.lines.remove(self.row);
        self.row -= 1;
        self.col = self.lines[self.row].len();
        self.lines[self.row].push_str(&current);
    }

    pub fn delete_forward(&mut self) {
        let line_len = self.lines[self.row].len();
        if self.col < line_len {
            let next = next_char_boundary(&self.lines[self.row], self.col);
            self.lines[self.row].replace_range(self.col..next, "");
            return;
        }
        if self.row + 1 >= self.lines.len() {
            return;
        }
        let next = self.lines.remove(self.row + 1);
        self.lines[self.row].push_str(&next);
    }

    pub fn kill_to_end(&mut self) {
        self.lines[self.row].truncate(self.col);
    }

    pub fn kill_to_start(&mut self) {
        self.lines[self.row].replace_range(..self.col, "");
        self.col = 0;
    }

    pub fn delete_word_back(&mut self) {
        if self.col == 0 {
            self.backspace();
            return;
        }
        let target = prev_word_boundary(&self.lines[self.row], self.col);
        self.lines[self.row].replace_range(target..self.col, "");
        self.col = target;
    }

    pub fn move_left(&mut self) {
        if self.col > 0 {
            self.col = prev_char_boundary(&self.lines[self.row], self.col);
            return;
        }
        if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].len();
        }
    }

    pub fn move_right(&mut self) {
        let line_len = self.lines[self.row].len();
        if self.col < line_len {
            self.col = next_char_boundary(&self.lines[self.row], self.col);
            return;
        }
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.row == 0 {
            self.col = 0;
            return;
        }
        self.row -= 1;
        self.col = clamp_to_boundary(&self.lines[self.row], self.col);
    }

    pub fn move_down(&mut self) {
        if self.row + 1 >= self.lines.len() {
            self.col = self.lines[self.row].len();
            return;
        }
        self.row += 1;
        self.col = clamp_to_boundary(&self.lines[self.row], self.col);
    }

    pub fn move_home(&mut self) {
        self.col = 0;
    }

    pub fn move_end(&mut self) {
        self.col = self.lines[self.row].len();
    }

    pub fn move_doc_start(&mut self) {
        self.row = 0;
        self.col = 0;
    }

    pub fn move_doc_end(&mut self) {
        self.row = self.lines.len() - 1;
        self.col = self.lines[self.row].len();
    }

    pub fn move_word_back(&mut self) {
        if self.col == 0 {
            if self.row > 0 {
                self.row -= 1;
                self.col = self.lines[self.row].len();
            }
            return;
        }
        self.col = prev_word_boundary(&self.lines[self.row], self.col);
    }

    /// Kill from cursor to the end of the current word (Alt+D in Emacs).
    pub fn kill_word_forward(&mut self) {
        let line_len = self.lines[self.row].len();
        if self.col >= line_len {
            self.delete_forward();
            return;
        }
        let target = next_word_boundary(&self.lines[self.row], self.col);
        self.lines[self.row].replace_range(self.col..target, "");
    }

    pub fn move_word_forward(&mut self) {
        let line_len = self.lines[self.row].len();
        if self.col >= line_len {
            if self.row + 1 < self.lines.len() {
                self.row += 1;
                self.col = 0;
            }
            return;
        }
        self.col = next_word_boundary(&self.lines[self.row], self.col);
    }
}

fn prev_char_boundary(s: &str, from: usize) -> usize {
    let mut i = from.saturating_sub(1);
    'walk: while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
        continue 'walk;
    }
    i
}

fn next_char_boundary(s: &str, from: usize) -> usize {
    let mut i = from + 1;
    'walk: while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
        continue 'walk;
    }
    i.min(s.len())
}

fn clamp_to_boundary(s: &str, col: usize) -> usize {
    if col >= s.len() {
        return s.len();
    }
    let mut i = col;
    'walk: while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
        continue 'walk;
    }
    i
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn prev_word_boundary(s: &str, from: usize) -> usize {
    let mut i = from;
    // Walk left over non-word chars.
    'skip_gap: loop {
        if i == 0 {
            return 0;
        }
        let prev = prev_char_boundary(s, i);
        let Some(c) = s[prev..i].chars().next() else {
            return prev;
        };
        if is_word_char(c) {
            break 'skip_gap;
        }
        i = prev;
        continue 'skip_gap;
    }
    // Walk left over the word itself.
    'skip_word: loop {
        if i == 0 {
            return 0;
        }
        let prev = prev_char_boundary(s, i);
        let Some(c) = s[prev..i].chars().next() else {
            return prev;
        };
        if !is_word_char(c) {
            return i;
        }
        i = prev;
        continue 'skip_word;
    }
}

fn next_word_boundary(s: &str, from: usize) -> usize {
    let len = s.len();
    let mut i = from;
    // Walk right over non-word chars.
    'skip_gap: loop {
        if i >= len {
            return len;
        }
        let Some(c) = s[i..].chars().next() else {
            return i;
        };
        if is_word_char(c) {
            break 'skip_gap;
        }
        i += c.len_utf8();
        continue 'skip_gap;
    }
    // Walk right over the word itself.
    'skip_word: loop {
        if i >= len {
            return len;
        }
        let Some(c) = s[i..].chars().next() else {
            return i;
        };
        if !is_word_char(c) {
            return i;
        }
        i += c.len_utf8();
        continue 'skip_word;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed_at(text: &str) -> Editor {
        let mut e = Editor::new();
        e.set_text(text);
        e
    }

    #[test]
    fn default_has_one_empty_line() {
        let e = Editor::new();
        assert!(e.is_empty());
        assert_eq!(e.line_count(), 1);
        assert_eq!(e.cursor(), (0, 0));
    }

    #[test]
    fn insert_char_appends_ascii() {
        let mut e = Editor::new();
        e.insert_char('h');
        e.insert_char('i');
        assert_eq!(e.text(), "hi");
        assert_eq!(e.cursor(), (0, 2));
        assert_eq!(e.visual_col(), 2);
    }

    #[test]
    fn insert_char_handles_multibyte() {
        let mut e = Editor::new();
        e.insert_char('ё');
        e.insert_char('ж');
        assert_eq!(e.text(), "ёж");
        // Each Cyrillic char is 2 bytes; cursor should be at byte 4.
        assert_eq!(e.cursor(), (0, 4));
        assert_eq!(e.visual_col(), 2);
    }

    #[test]
    fn insert_newline_splits_line() {
        let mut e = ed_at("hello world");
        // Move cursor to after "hello".
        e.move_home();
        for _ in 0..5 {
            e.move_right();
        }
        e.insert_newline();
        assert_eq!(e.text(), "hello\n world");
        assert_eq!(e.cursor(), (1, 0));
    }

    #[test]
    fn insert_str_splits_on_newlines() {
        let mut e = Editor::new();
        e.insert_str("line1\nline2\nline3");
        assert_eq!(e.text(), "line1\nline2\nline3");
        assert_eq!(e.line_count(), 3);
        assert_eq!(e.cursor(), (2, 5));
    }

    #[test]
    fn insert_str_into_existing_line() {
        let mut e = ed_at("prefix suffix");
        // Place cursor after "prefix ".
        e.move_home();
        for _ in 0.."prefix ".len() {
            e.move_right();
        }
        e.insert_str("A\nB");
        assert_eq!(e.text(), "prefix A\nBsuffix");
    }

    #[test]
    fn insert_str_trailing_newline_creates_empty_line() {
        let mut e = Editor::new();
        e.insert_str("x\n");
        assert_eq!(e.line_count(), 2);
        assert_eq!(e.lines()[0], "x");
        assert_eq!(e.lines()[1], "");
    }

    #[test]
    fn backspace_within_line() {
        let mut e = ed_at("abc");
        e.backspace();
        assert_eq!(e.text(), "ab");
        assert_eq!(e.cursor(), (0, 2));
    }

    #[test]
    fn backspace_at_col_zero_joins_previous_line() {
        let mut e = ed_at("foo\nbar");
        // Cursor is at end of "bar"; move to start of line 1.
        e.move_home();
        assert_eq!(e.cursor(), (1, 0));
        e.backspace();
        assert_eq!(e.text(), "foobar");
        assert_eq!(e.cursor(), (0, 3));
    }

    #[test]
    fn backspace_at_doc_start_noop() {
        let mut e = Editor::new();
        e.backspace();
        assert!(e.is_empty());
    }

    #[test]
    fn backspace_multibyte() {
        let mut e = ed_at("日本");
        e.backspace();
        assert_eq!(e.text(), "日");
    }

    #[test]
    fn delete_forward_joins_next_line_at_eol() {
        let mut e = ed_at("foo\nbar");
        e.move_doc_start();
        e.move_end();
        e.delete_forward();
        assert_eq!(e.text(), "foobar");
        assert_eq!(e.cursor(), (0, 3));
    }

    #[test]
    fn kill_to_end_truncates_line() {
        let mut e = ed_at("hello world");
        e.move_home();
        for _ in 0..5 {
            e.move_right();
        }
        e.kill_to_end();
        assert_eq!(e.text(), "hello");
    }

    #[test]
    fn kill_to_start_removes_line_prefix() {
        let mut e = ed_at("hello world");
        e.move_home();
        for _ in 0..6 {
            e.move_right();
        }
        e.kill_to_start();
        assert_eq!(e.text(), "world");
        assert_eq!(e.cursor(), (0, 0));
    }

    #[test]
    fn delete_word_back_removes_last_word() {
        let mut e = ed_at("one two three");
        e.delete_word_back();
        assert_eq!(e.text(), "one two ");
    }

    #[test]
    fn delete_word_back_skips_leading_space() {
        let mut e = ed_at("one two   ");
        e.delete_word_back();
        assert_eq!(e.text(), "one ");
    }

    #[test]
    fn delete_word_back_across_line_boundary_falls_back_to_backspace() {
        let mut e = ed_at("a\nb");
        e.move_home();
        e.delete_word_back();
        assert_eq!(e.text(), "ab");
    }

    #[test]
    fn move_left_wraps_to_previous_line() {
        let mut e = ed_at("ab\ncd");
        e.move_home();
        e.move_left();
        assert_eq!(e.cursor(), (0, 2));
    }

    #[test]
    fn move_right_wraps_to_next_line() {
        let mut e = ed_at("ab\ncd");
        e.move_doc_start();
        e.move_end();
        e.move_right();
        assert_eq!(e.cursor(), (1, 0));
    }

    #[test]
    fn move_up_down_clamps_column() {
        let mut e = ed_at("long line\nshort");
        // Place cursor at end of line 1 ("short", col=5).
        e.move_end();
        e.move_up();
        // Column should be clamped; we were at 5, line 0 has 9 bytes so 5 stays.
        assert_eq!(e.cursor(), (0, 5));
        // Make top line shorter via a fresh editor.
        let mut e = ed_at("ab\nXXXXXXX");
        e.move_end();
        e.move_up();
        assert_eq!(e.cursor(), (0, 2));
    }

    #[test]
    fn move_home_end_navigate_current_line() {
        let mut e = ed_at("abcdef");
        e.move_home();
        assert_eq!(e.cursor(), (0, 0));
        e.move_end();
        assert_eq!(e.cursor(), (0, 6));
    }

    #[test]
    fn move_doc_end_goes_to_last_line_end() {
        let mut e = ed_at("a\nb\nc");
        e.move_doc_start();
        e.move_doc_end();
        assert_eq!(e.cursor(), (2, 1));
    }

    #[test]
    fn move_word_back_across_punctuation() {
        let mut e = ed_at("foo.bar baz");
        // cursor at end
        e.move_word_back();
        assert_eq!(&e.lines()[0][..e.cursor().1], "foo.bar ");
        e.move_word_back();
        assert_eq!(&e.lines()[0][..e.cursor().1], "foo.");
        e.move_word_back();
        assert_eq!(&e.lines()[0][..e.cursor().1], "");
    }

    #[test]
    fn move_word_forward_across_punctuation() {
        let mut e = ed_at("foo.bar baz");
        e.move_doc_start();
        e.move_word_forward();
        assert_eq!(&e.lines()[0][..e.cursor().1], "foo");
        e.move_word_forward();
        assert_eq!(&e.lines()[0][..e.cursor().1], "foo.bar");
        e.move_word_forward();
        assert_eq!(&e.lines()[0][..e.cursor().1], "foo.bar baz");
    }

    #[test]
    fn move_word_back_at_line_start_jumps_up() {
        let mut e = ed_at("hello\nworld");
        e.move_home();
        e.move_word_back();
        assert_eq!(e.cursor(), (0, 5));
    }

    #[test]
    fn ends_with_backslash_detection() {
        let mut e = ed_at("hi \\");
        assert!(e.ends_with_backslash());
        e.pop_trailing_backslash();
        assert!(!e.ends_with_backslash());
        assert_eq!(e.text(), "hi ");
    }

    #[test]
    fn pop_trailing_backslash_rewinds_cursor_if_at_end() {
        let mut e = ed_at("foo\\");
        assert_eq!(e.cursor(), (0, 4));
        e.pop_trailing_backslash();
        assert_eq!(e.cursor(), (0, 3));
        assert_eq!(e.text(), "foo");
    }

    #[test]
    fn set_text_resets_and_places_cursor_at_end() {
        let mut e = Editor::new();
        e.set_text("a\nbb");
        assert_eq!(e.cursor(), (1, 2));
        assert_eq!(e.line_count(), 2);
    }

    #[test]
    fn clear_returns_to_single_empty_line() {
        let mut e = ed_at("foo\nbar");
        e.clear();
        assert!(e.is_empty());
        assert_eq!(e.cursor(), (0, 0));
    }

    #[test]
    fn kill_word_forward_removes_word() {
        let mut e = ed_at("hello world");
        e.move_home();
        e.kill_word_forward();
        assert_eq!(e.text(), " world");
    }

    #[test]
    fn kill_word_forward_at_eol_joins_next_line() {
        let mut e = ed_at("foo\nbar");
        e.move_doc_start();
        e.move_end();
        e.kill_word_forward();
        assert_eq!(e.text(), "foobar");
    }

    #[test]
    fn kill_word_forward_skips_space_then_kills_word() {
        let mut e = ed_at("one  two");
        e.move_home();
        for _ in 0..3 {
            e.move_right();
        }
        e.kill_word_forward();
        assert_eq!(e.text(), "one");
    }

    #[test]
    fn text_single_line_collapses_newlines() {
        let mut e = Editor::new();
        e.insert_str("a\nb\nc");
        assert_eq!(e.text_single_line(), "a b c");
    }
}
