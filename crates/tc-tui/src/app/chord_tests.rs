use crossterm::event::{KeyCode, KeyModifiers};

use crate::event::Message;
use crate::keybind::PendingChord;

use super::settings::SettingsField;
use super::test_support::{app_with, dummy_task};
use super::types::{FocusPanel, InputMode};

fn key(c: char) -> (KeyCode, KeyModifiers) {
    (KeyCode::Char(c), KeyModifiers::NONE)
}

#[test]
fn ctrl_w_then_l_moves_focus_right() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.focus = FocusPanel::Epics;
    app.update(Message::Key(KeyCode::Char('w'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.pending_chord, PendingChord::CtrlW);
    app.update(Message::Key(key('l').0, key('l').1)).unwrap();
    assert_eq!(app.focus, FocusPanel::Tasks);
    assert_eq!(app.pending_chord, PendingChord::None);
}

#[test]
fn space_w_l_moves_focus_right() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.focus = FocusPanel::Epics;
    app.update(Message::Key(key(' ').0, key(' ').1)).unwrap();
    assert_eq!(app.pending_chord, PendingChord::Leader);
    app.update(Message::Key(key('w').0, key('w').1)).unwrap();
    assert_eq!(app.pending_chord, PendingChord::LeaderWindow);
    app.update(Message::Key(key('l').0, key('l').1)).unwrap();
    assert_eq!(app.focus, FocusPanel::Tasks);
    assert_eq!(app.pending_chord, PendingChord::None);
}

#[test]
fn space_v_l_toggles_log() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    assert!(!app.show_log);
    app.update(Message::Key(key(' ').0, key(' ').1)).unwrap();
    app.update(Message::Key(key('v').0, key('v').1)).unwrap();
    assert_eq!(app.pending_chord, PendingChord::LeaderView);
    app.update(Message::Key(key('l').0, key('l').1)).unwrap();
    assert!(app.show_log);
    assert_eq!(app.pending_chord, PendingChord::None);
}

#[test]
fn esc_cancels_chord() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(key(' ').0, key(' ').1)).unwrap();
    assert_eq!(app.pending_chord, PendingChord::Leader);
    app.update(Message::Key(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.pending_chord, PendingChord::None);
    // Esc from no chord should quit
    app.update(Message::Key(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(!app.running);
}

#[test]
fn which_key_hidden_before_delay() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(key(' ').0, key(' ').1)).unwrap();
    // Immediately after pressing space, popup should not yet show.
    assert!(app.which_key_chord().is_none());
    // wake timer should be in the future.
    assert!(app.chord_wake_in().is_some());
}

#[test]
fn pager_j_scrolls_log_when_focused() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.focus = FocusPanel::Log;
    app.log_view.viewport_height.set(5);
    app.log_view
        .set_lines((0..20).map(|i| format!("line {i}")).collect());
    app.log_view.scroll_to_top();
    assert_eq!(app.log_view.offset, 0);
    app.update(Message::Key(KeyCode::Char('j'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.log_view.offset, 1);
    assert!(!app.log_view.follow);
}

#[test]
fn pager_slash_enters_log_search_mode() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.focus = FocusPanel::Log;
    app.update(Message::Key(KeyCode::Char('/'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.input_mode, InputMode::LogSearch);
}

#[test]
fn slash_outside_log_opens_filter() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.focus = FocusPanel::Tasks;
    app.update(Message::Key(KeyCode::Char('/'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.input_mode, InputMode::Filter);
}

#[test]
fn pager_shift_g_enables_follow() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.focus = FocusPanel::Log;
    app.log_view.viewport_height.set(5);
    app.log_view
        .set_lines((0..20).map(|i| format!("line {i}")).collect());
    app.log_view.scroll_up(5);
    assert!(!app.log_view.follow);
    app.update(Message::Key(KeyCode::Char('G'), KeyModifiers::SHIFT))
        .unwrap();
    assert!(app.log_view.follow);
}

#[test]
fn toggle_log_off_resets_focus() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.show_log = true;
    app.focus = FocusPanel::Log;
    // SPC v l -> toggles log off; focus should move back to Tasks.
    app.update(Message::Key(key(' ').0, key(' ').1)).unwrap();
    app.update(Message::Key(key('v').0, key('v').1)).unwrap();
    app.update(Message::Key(key('l').0, key('l').1)).unwrap();
    assert!(!app.show_log);
    assert_eq!(app.focus, FocusPanel::Tasks);
}

#[test]
fn comma_opens_settings_popup() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    assert!(app.settings.is_none());
    app.update(Message::Key(KeyCode::Char(','), KeyModifiers::NONE))
        .unwrap();
    let state = app.settings.as_ref().expect("settings open");
    assert_eq!(state.executor, tc_core::config::ExecutorKind::Claude);
    assert_eq!(state.mode, tc_core::config::ExecutionMode::Accept);
    assert_eq!(state.field, SettingsField::Executor);
    assert!(!state.dirty);
}

#[test]
fn settings_l_cycles_executor_and_marks_dirty() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char(','), KeyModifiers::NONE))
        .unwrap();
    app.update(Message::Key(KeyCode::Char('l'), KeyModifiers::NONE))
        .unwrap();
    let state = app.settings.as_ref().expect("settings open");
    assert_ne!(state.executor, tc_core::config::ExecutorKind::Claude);
    assert!(state.dirty);
}

#[test]
fn settings_j_moves_to_mode_field() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char(','), KeyModifiers::NONE))
        .unwrap();
    app.update(Message::Key(KeyCode::Char('j'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.settings.as_ref().unwrap().field, SettingsField::Mode);
}

#[test]
fn settings_esc_closes_without_saving() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char(','), KeyModifiers::NONE))
        .unwrap();
    app.update(Message::Key(KeyCode::Char('l'), KeyModifiers::NONE))
        .unwrap();
    app.update(Message::Key(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.settings.is_none());
    assert_eq!(
        app.config.executor.default,
        tc_core::config::ExecutorKind::Claude
    );
}

#[test]
fn leader_capital_s_opens_settings() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(key(' ').0, key(' ').1)).unwrap();
    app.update(Message::Key(KeyCode::Char('S'), KeyModifiers::SHIFT))
        .unwrap();
    assert!(app.settings.is_some());
}
