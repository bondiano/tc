use crossterm::event::{KeyCode, KeyModifiers};

use crate::event::Message;
use crate::keybind::PendingChord;

use super::test_support::{app_with, dummy_task};
use super::types::{App, InputMode};
use super::ui_state::write_image_attachment;

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.update(Message::Key(code, mods)).unwrap();
}

fn enter_add(app: &mut App) {
    press(app, KeyCode::Char('A'), KeyModifiers::SHIFT);
    assert_eq!(app.input_mode, InputMode::AddTask);
}

#[test]
fn typing_inserts_chars_into_editor() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('h'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('i'), KeyModifiers::NONE);
    assert_eq!(app.input.text(), "hi");
}

#[test]
fn shift_enter_inserts_newline_and_does_not_submit() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Char('b'), KeyModifiers::NONE);
    assert_eq!(app.input_mode, InputMode::AddTask, "still in input mode");
    assert_eq!(app.input.text(), "a\nb");
}

#[test]
fn alt_enter_inserts_newline() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::ALT);
    assert_eq!(app.input_mode, InputMode::AddTask);
    assert_eq!(app.input.text(), "x\n");
}

#[test]
fn trailing_backslash_plus_enter_inserts_newline() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('\\'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.input_mode, InputMode::AddTask);
    assert_eq!(app.input.text(), "a\n");
}

#[test]
fn plain_enter_submits_add_task() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('n'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('e'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('w'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.input_mode, InputMode::Normal);
    assert!(app.input.is_empty());
}

#[test]
fn ctrl_k_truncates_to_end_of_line() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('h'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('e'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('l'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('l'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    for _ in 0..2 {
        press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert_eq!(app.input.text(), "he");
}

#[test]
fn ctrl_u_kills_to_start_of_line() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    for c in "hello".chars() {
        press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(app.input.text(), "");
}

#[test]
fn ctrl_a_and_ctrl_e_jump_line_ends() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    for c in "hi".chars() {
        press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert_eq!(app.input.cursor(), (0, 0));
    press(&mut app, KeyCode::Char('e'), KeyModifiers::CONTROL);
    assert_eq!(app.input.cursor(), (0, 2));
}

#[test]
fn alt_b_moves_word_back() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    for c in "one two".chars() {
        press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
    assert_eq!(app.input.cursor(), (0, 4));
}

#[test]
fn paste_message_is_inserted_at_cursor() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    app.update(Message::Paste("yz".into())).unwrap();
    assert_eq!(app.input.text(), "xyz");
}

#[test]
fn paste_in_filter_mode_collapses_newlines() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
    assert_eq!(app.input_mode, InputMode::Filter);
    app.update(Message::Paste("foo\nbar".into())).unwrap();
    assert_eq!(app.filter, "foo bar");
    assert!(!app.input.text().contains('\n'));
}

#[test]
fn paste_in_normal_mode_is_ignored() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Paste("should not appear".into()))
        .unwrap();
    assert!(app.input.is_empty());
    assert_eq!(app.filter, "");
}

#[test]
fn esc_clears_input_and_returns_to_normal() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    press(&mut app, KeyCode::Char('h'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.input_mode, InputMode::Normal);
    assert!(app.input.is_empty());
}

#[test]
fn filter_live_syncs_on_char() {
    let mut app = app_with(vec![
        dummy_task("T-001", "alpha", "todo"),
        dummy_task("T-002", "alpha", "todo"),
    ]);
    press(&mut app, KeyCode::Char('/'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('T'), KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Char('-'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('0'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('0'), KeyModifiers::NONE);
    press(&mut app, KeyCode::Char('1'), KeyModifiers::NONE);
    assert_eq!(app.filter, "T-001");
    assert_eq!(app.visible_tasks().len(), 1);
}

#[test]
fn ctrl_w_in_input_mode_deletes_word_not_window_chord() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    enter_add(&mut app);
    for c in "one two".chars() {
        press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Char('w'), KeyModifiers::CONTROL);
    assert_eq!(app.input.text(), "one ");
    // Should still be in input mode; no CtrlW chord pending.
    assert_eq!(app.input_mode, InputMode::AddTask);
    assert_eq!(app.pending_chord, PendingChord::None);
}

#[test]
fn write_image_attachment_creates_file() {
    use tempfile::TempDir;
    let td = TempDir::new().unwrap();
    let bytes = vec![0u8; 4]; // 1x1 RGBA black pixel
    let image = arboard::ImageData {
        width: 1,
        height: 1,
        bytes: bytes.into(),
    };
    let rel = write_image_attachment(td.path(), &image).expect("write");
    let abs = td.path().join(&rel);
    assert!(abs.exists(), "image written at {abs:?}");
    assert!(rel.starts_with(".tc/attachments/inbox"));
    let written = std::fs::read(&abs).unwrap();
    // PNG magic bytes.
    assert_eq!(&written[..8], b"\x89PNG\r\n\x1a\n");
}
