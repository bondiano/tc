//! Unit tests for `App::update` -- the Elm-style Message -> state transition.
//!
//! These complement `chord_tests` and `input_tests` (which focus on specific
//! key sequences) by covering the shape of each `Message` variant.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::create_task::CreateTaskField;
use crate::event::Message;
use crate::keybind::PendingChord;

use super::test_support::{app_with, dummy_task};
use super::types::{AppScreen, FocusPanel, InputMode};

#[test]
fn quit_message_stops_running() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    assert!(app.running);
    app.update(Message::Quit).unwrap();
    assert!(!app.running);
}

#[test]
fn quit_while_editing_still_stops_running() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char('A'), KeyModifiers::SHIFT))
        .unwrap();
    app.update(Message::Key(KeyCode::Char('d'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.input_mode, InputMode::AddTask);
    app.update(Message::Quit).unwrap();
    assert!(!app.running);
}

#[test]
fn tick_is_a_noop_when_nothing_changes() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    let before = app.status_message.clone();
    app.update(Message::Tick).unwrap();
    // Tick should not change status without worker transitions.
    assert_eq!(app.status_message, before);
    assert!(app.running);
}

#[test]
fn key_message_is_dispatched_to_input_layer() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char('A'), KeyModifiers::SHIFT))
        .unwrap();
    assert_eq!(app.input_mode, InputMode::AddTask);
}

#[test]
fn paste_in_create_task_title_inserts_into_form_field() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.open_create_task_form_for_test();
    assert_eq!(app.screen, AppScreen::CreateTask);
    app.update(Message::Paste("hello".into())).unwrap();
    let form = app.create_task_form.as_ref().expect("form open");
    assert_eq!(form.title.text_single_line(), "hello");
}

#[test]
fn paste_in_create_task_depends_on_collapses_newlines() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.open_create_task_form_for_test();
    if let Some(form) = app.create_task_form.as_mut() {
        form.active_field = CreateTaskField::DependsOn;
    }
    app.update(Message::Paste("T-1\nT-2".into())).unwrap();
    let form = app.create_task_form.as_ref().expect("form open");
    assert_eq!(form.dep_input.text_single_line(), "T-1 T-2");
}

#[test]
fn paste_outside_input_mode_is_dropped() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    assert_eq!(app.input_mode, InputMode::Normal);
    app.update(Message::Paste("ignored".into())).unwrap();
    assert!(app.input.is_empty());
    assert!(app.filter.is_empty());
}

#[test]
fn paste_in_add_task_inserts_at_cursor() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char('A'), KeyModifiers::SHIFT))
        .unwrap();
    app.update(Message::Paste("abc".into())).unwrap();
    assert_eq!(app.input.text(), "abc");
}

#[test]
fn paste_in_log_search_collapses_newlines() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.focus = FocusPanel::Log;
    app.update(Message::Key(KeyCode::Char('/'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.input_mode, InputMode::LogSearch);
    app.update(Message::Paste("foo\nbar".into())).unwrap();
    assert!(!app.input.text().contains('\n'));
    assert_eq!(app.input.text_single_line(), "foo bar");
}

#[test]
fn click_message_is_accepted() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    // Click outside any interactive region should not crash or change running.
    app.update(Message::Click { col: 0, row: 0 }).unwrap();
    assert!(app.running);
}

#[test]
fn update_chains_messages_into_a_leader_chord() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char(' '), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.pending_chord, PendingChord::Leader);
    app.update(Message::Key(KeyCode::Char('v'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.pending_chord, PendingChord::LeaderView);
    app.update(Message::Key(KeyCode::Char('g'), KeyModifiers::NONE))
        .unwrap();
    assert!(app.show_dag);
    assert_eq!(app.pending_chord, PendingChord::None);
}

#[test]
fn quit_key_stops_running() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char('q'), KeyModifiers::NONE))
        .unwrap();
    assert!(!app.running);
}

impl super::types::App {
    fn open_create_task_form_for_test(&mut self) {
        use crate::create_task::CreateTaskForm;
        self.create_task_form = Some(CreateTaskForm::new("test".into()));
        self.screen = super::types::AppScreen::CreateTask;
    }
}
