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

// ── M-7.6: fullscreen edit modal ────────────────────────────────────

#[test]
fn pressing_e_opens_edit_form_prefilled_from_selected_task() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char('e'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.screen, AppScreen::CreateTask);
    let form = app.create_task_form.as_ref().expect("form open");
    assert!(form.is_editing(), "form should be in edit mode");
    assert_eq!(form.editing.as_ref().map(|i| i.0.as_str()), Some("T-001"));
    assert_eq!(form.title.text_single_line(), "Title for T-001");
    assert_eq!(form.epic.text_single_line(), "alpha");
}

#[test]
fn pressing_e_with_no_task_toasts_and_stays_on_main() {
    let mut app = app_with(vec![]);
    app.update(Message::Key(KeyCode::Char('e'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.screen, AppScreen::Main);
    assert!(app.create_task_form.is_none());
    assert!(
        app.status_message.contains("no task selected"),
        "toast: {}",
        app.status_message
    );
}

#[test]
fn leader_t_e_also_opens_edit_form() {
    let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
    app.update(Message::Key(KeyCode::Char(' '), KeyModifiers::NONE))
        .unwrap();
    app.update(Message::Key(KeyCode::Char('t'), KeyModifiers::NONE))
        .unwrap();
    app.update(Message::Key(KeyCode::Char('e'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.screen, AppScreen::CreateTask);
    let form = app.create_task_form.as_ref().expect("form open");
    assert!(form.is_editing());
}

impl super::types::App {
    fn open_create_task_form_for_test(&mut self) {
        use crate::create_task::CreateTaskForm;
        self.create_task_form = Some(CreateTaskForm::new("test".into()));
        self.screen = super::types::AppScreen::CreateTask;
    }
}

#[cfg(test)]
mod smart_view_and_fuzzy {
    //! Tests for M-7.1 (fuzzy ranking) and M-7.2 (smart-view tabs).

    use chrono::{Days, Local};
    use crossterm::event::{KeyCode, KeyModifiers};

    use crate::app::test_support::{app_with, dummy_task};
    use crate::app::types::SmartView;
    use crate::event::Message;

    fn press(app: &mut crate::app::App, code: KeyCode) {
        app.update(Message::Key(code, KeyModifiers::NONE)).unwrap();
    }

    #[test]
    fn number_keys_switch_smart_view() {
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        assert_eq!(app.smart_view, SmartView::All);
        press(&mut app, KeyCode::Char('1'));
        assert_eq!(app.smart_view, SmartView::Today);
        press(&mut app, KeyCode::Char('2'));
        assert_eq!(app.smart_view, SmartView::Upcoming);
        press(&mut app, KeyCode::Char('3'));
        assert_eq!(app.smart_view, SmartView::Inbox);
        press(&mut app, KeyCode::Char('4'));
        assert_eq!(app.smart_view, SmartView::All);
    }

    #[test]
    fn inbox_view_hides_tasks_with_due_date() {
        let mut with_due = dummy_task("T-001", "alpha", "todo");
        with_due.due = Some(Local::now().date_naive());
        let no_due = dummy_task("T-002", "alpha", "todo");
        let mut app = app_with(vec![with_due, no_due]);
        app.set_smart_view(SmartView::Inbox);
        let visible: Vec<String> = app.visible_tasks().iter().map(|t| t.id.0.clone()).collect();
        assert_eq!(visible, vec!["T-002".to_string()]);
    }

    #[test]
    fn today_view_includes_due_today() {
        let today = Local::now().date_naive();
        let mut due_today = dummy_task("T-001", "alpha", "todo");
        due_today.due = Some(today);
        let due_later = {
            let mut t = dummy_task("T-002", "alpha", "todo");
            t.due = today.checked_add_days(Days::new(3));
            t
        };
        let mut app = app_with(vec![due_today, due_later]);
        app.set_smart_view(SmartView::Today);
        let visible: Vec<String> = app.visible_tasks().iter().map(|t| t.id.0.clone()).collect();
        assert_eq!(visible, vec!["T-001".to_string()]);
    }

    #[test]
    fn upcoming_view_excludes_today_and_far_future() {
        let today = Local::now().date_naive();
        let mut due_today = dummy_task("T-001", "alpha", "todo");
        due_today.due = Some(today);
        let mut due_in_3 = dummy_task("T-002", "alpha", "todo");
        due_in_3.due = today.checked_add_days(Days::new(3));
        let mut due_in_30 = dummy_task("T-003", "alpha", "todo");
        due_in_30.due = today.checked_add_days(Days::new(30));
        let mut app = app_with(vec![due_today, due_in_3, due_in_30]);
        app.set_smart_view(SmartView::Upcoming);
        let visible: Vec<String> = app.visible_tasks().iter().map(|t| t.id.0.clone()).collect();
        assert_eq!(visible, vec!["T-002".to_string()]);
    }

    #[test]
    fn fuzzy_query_ranks_better_match_first() {
        // "fuzz" should match both T-001 and T-003, but the tighter match
        // should rank ahead. T-002 has nothing relevant and must be filtered.
        let mut t1 = dummy_task("T-001", "alpha", "todo");
        t1.title = "fuzzy match search engine".into();
        let mut t2 = dummy_task("T-002", "alpha", "todo");
        t2.title = "unrelated task name".into();
        let mut t3 = dummy_task("T-003", "alpha", "todo");
        t3.title = "fuzz".into();
        let mut app = app_with(vec![t1, t2, t3]);
        app.filter = "fuzz".into();
        let visible: Vec<String> = app.visible_tasks().iter().map(|t| t.id.0.clone()).collect();
        assert!(!visible.contains(&"T-002".to_string()));
        assert!(visible.contains(&"T-001".to_string()));
        assert!(visible.contains(&"T-003".to_string()));
    }

    #[test]
    fn esc_clears_active_fuzzy_filter() {
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(app.filter, "x");
        press(&mut app, KeyCode::Esc);
        assert!(app.filter.is_empty(), "Esc should clear fuzzy query");
    }

    #[test]
    fn re_entering_fuzzy_mode_restores_previous_query() {
        // Press / to commit "abc" via Enter, then reopen with / -- the
        // editor should be pre-populated for refinement (M-7.1).
        let mut app = app_with(vec![dummy_task("T-001", "alpha", "todo")]);
        press(&mut app, KeyCode::Char('/'));
        for c in "abc".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter); // commit, exit input mode but keep query
        assert_eq!(app.filter, "abc");
        press(&mut app, KeyCode::Char('/'));
        assert_eq!(app.input.text(), "abc");
    }
}
