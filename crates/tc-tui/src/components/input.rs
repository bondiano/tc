use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, InputMode};

/// Minimum rows taken by the input pane (status bar). Border + 1 content row.
const MIN_ROWS: u16 = 3;
/// Clamp for multi-line modes. `Editor::max_rows` is advisory; this caps
/// the layout allocation so the input pane does not eat the whole screen.
const MAX_ROWS: u16 = 10;

pub fn render(app: &App, frame: &mut Frame<'_>, area: Rect) {
    let title = match app.input_mode {
        InputMode::AddTask => " Add Task (Enter submit / Shift+Enter newline / Esc cancel) ",
        InputMode::Filter => " Filter (Enter/Esc to exit) ",
        InputMode::Reject => " Reject (Enter submit / Shift+Enter newline / Esc cancel) ",
        InputMode::LogSearch => " Log search (Enter confirm / Esc clear) ",
        InputMode::Normal => " Status ",
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);

    if app.input_mode == InputMode::Normal {
        let p = Paragraph::new(app.status_message.clone()).block(block);
        frame.render_widget(p, area);
        return;
    }

    let style = Style::default().fg(Color::Yellow);
    let (row, _col) = app.input.cursor();
    let (top, visible_rows) = visible_window(app, inner.height);

    let lines: Vec<Line> = app
        .input
        .lines()
        .iter()
        .skip(top)
        .take(visible_rows)
        .map(|l| Line::from(l.clone()))
        .collect();

    let p = Paragraph::new(lines).style(style).block(block);
    frame.render_widget(p, area);

    // Terminal cursor: only drawn when the logical row is inside the
    // visible window. Clamp to the inner rect so it never lands on the
    // border.
    if row >= top && row < top + visible_rows {
        let visual_col = app.input.visual_col() as u16;
        let cx = inner.x + visual_col.min(inner.width.saturating_sub(1));
        let cy = inner.y + (row - top) as u16;
        frame.set_cursor_position(Position::new(cx, cy));
    }
}

/// How many rows the input pane should occupy in the vertical layout. For
/// Normal mode the pane is the 3-row status bar; for input modes the pane
/// grows with the number of editor lines, clamped to [MIN_ROWS, MAX_ROWS].
pub fn required_height(app: &App) -> u16 {
    if app.input_mode == InputMode::Normal {
        return MIN_ROWS;
    }
    let content = app.input.line_count() as u16;
    (content + 2).clamp(MIN_ROWS, MAX_ROWS)
}

/// Pick the top visible line so the cursor stays on screen. `inner_h` is
/// the inner (non-border) row count.
fn visible_window(app: &App, inner_h: u16) -> (usize, usize) {
    let inner = inner_h as usize;
    if inner == 0 {
        return (0, 0);
    }
    let (row, _) = app.input.cursor();
    let total = app.input.line_count();
    let visible = inner.min(total);
    if row < visible {
        return (0, visible);
    }
    let top = row + 1 - visible;
    let top = top.min(total.saturating_sub(visible));
    (top, visible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{app_with, dummy_task};
    use ratatui::Terminal;
    use ratatui::backend::{Backend, TestBackend};

    fn drive_keys(
        app: &mut App,
        keys: &[(crossterm::event::KeyCode, crossterm::event::KeyModifiers)],
    ) {
        for (code, mods) in keys {
            app.update(crate::event::Message::Key(*code, *mods))
                .unwrap();
        }
    }

    #[test]
    fn required_height_is_3_in_normal_mode() {
        let app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        assert_eq!(required_height(&app), MIN_ROWS);
    }

    #[test]
    fn required_height_grows_with_lines_in_add_task() {
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        app.input_mode = InputMode::AddTask;
        assert_eq!(required_height(&app), MIN_ROWS); // 1 line + 2 borders = 3
        app.input.insert_char('a');
        app.input.insert_newline();
        app.input.insert_char('b');
        // 2 lines + 2 borders = 4
        assert_eq!(required_height(&app), 4);
    }

    #[test]
    fn required_height_caps_at_max() {
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        app.input_mode = InputMode::AddTask;
        for _ in 0..20 {
            app.input.insert_newline();
        }
        assert_eq!(required_height(&app), MAX_ROWS);
    }

    #[test]
    fn cursor_position_matches_editor_in_add_task() {
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        app.input_mode = InputMode::AddTask;
        app.input.insert_str("hello");
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 40, 3);
                render(&app, f, area);
            })
            .unwrap();
        // Cursor should be at column 1 + visual_col (=5), row 1 (inside border).
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!(cursor.x, 1 + 5);
        assert_eq!(cursor.y, 1);
    }

    #[test]
    fn cursor_scrolls_when_content_exceeds_inner_height() {
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        app.input_mode = InputMode::AddTask;
        for i in 0..8 {
            app.input.insert_str(&format!("line {i}"));
            if i < 7 {
                app.input.insert_newline();
            }
        }
        // Cursor is at end of last line. Inner height of 3 means only 3 lines
        // are visible; cursor should sit on the last visible row.
        let (top, visible) = visible_window(&app, 3);
        assert_eq!(visible, 3);
        assert_eq!(top, 5); // 8 - 3
        let (row, _) = app.input.cursor();
        assert!(row >= top && row < top + visible);
    }

    #[test]
    fn render_shows_input_text() {
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        app.input_mode = InputMode::AddTask;
        drive_keys(
            &mut app,
            &[
                (
                    crossterm::event::KeyCode::Char('h'),
                    crossterm::event::KeyModifiers::NONE,
                ),
                (
                    crossterm::event::KeyCode::Char('i'),
                    crossterm::event::KeyModifiers::NONE,
                ),
            ],
        );
        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let area = Rect::new(0, 0, 40, 3);
                render(&app, f, area);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut row1 = String::new();
        for x in 0..buf.area.width {
            row1.push_str(buf[(x, 1)].symbol());
        }
        assert!(row1.contains("hi"), "rendered row: {row1:?}");
    }
}
