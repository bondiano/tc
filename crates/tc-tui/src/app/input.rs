use crossterm::event::{KeyCode, KeyModifiers};
use tc_core::status::StatusId;
use tc_core::task::Task;

use crate::create_task::{CreateTaskField, CreateTaskForm};
use crate::editor::Editor;
use crate::error::TuiResult;
use crate::keybind::{Direction, PendingChord};

use super::ALL_EPIC;
use super::types::{App, AppScreen, FocusPanel, InputMode, SmartView};

impl App {
    pub(super) fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) -> TuiResult<()> {
        if self.screen == AppScreen::CreateTask {
            return self.on_key_create_task(code, mods);
        }
        if self.input_mode != InputMode::Normal {
            return self.on_key_input(code, mods);
        }
        self.on_key_normal(code, mods)
    }

    fn on_key_normal(&mut self, code: KeyCode, mods: KeyModifiers) -> TuiResult<()> {
        if self.pending_delete.is_some() {
            return self.on_key_pending_delete(code);
        }

        if self.show_task_card {
            self.on_key_task_card(code);
            return Ok(());
        }

        if self.show_help {
            match code {
                KeyCode::Char('q') => self.running = false,
                _ => self.show_help = false,
            }
            return Ok(());
        }

        if self.settings.is_some() {
            return self.on_key_settings(code);
        }

        if self.pending_chord.is_active() {
            return self.on_key_chord(code, mods);
        }

        if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('w')) {
            self.begin_chord(PendingChord::CtrlW);
            return Ok(());
        }
        if matches!(code, KeyCode::Char(' ')) {
            self.begin_chord(PendingChord::Leader);
            return Ok(());
        }

        if self.focus == FocusPanel::Log && self.handle_log_pager_key(code, mods) {
            return Ok(());
        }

        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.running = false;
            }
            KeyCode::Tab => self.cycle_focus(),
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Enter => {
                if self.focus == FocusPanel::Tasks && self.selected_task().is_some() {
                    self.show_task_card = true;
                    self.task_card_scroll = 0;
                } else {
                    self.focus = FocusPanel::Tasks;
                }
            }
            KeyCode::Char('g') => self.toggle_dag(),
            KeyCode::Char('l') => self.toggle_log(),
            KeyCode::Char('a') => self.open_create_task_form(),
            KeyCode::Char('A') => self.enter_input(InputMode::AddTask, "Quick add task: "),
            KeyCode::Char('e') => self.open_edit_task_form(),
            KeyCode::Char('/') => self.enter_input(InputMode::Filter, "Filter: "),
            KeyCode::Char('d') => self.action_done()?,
            KeyCode::Char('x') => self.action_delete(),
            KeyCode::Char('i') => self.action_impl()?,
            KeyCode::Char('y') | KeyCode::Char('s') => self.action_spawn()?,
            KeyCode::Char('K') => self.action_kill()?,
            KeyCode::Char('r') => self.action_review()?,
            KeyCode::Char('R') => self.enter_input(InputMode::Reject, "Reject reason: "),
            KeyCode::Char('m') => self.action_merge()?,
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
            }
            KeyCode::Char(',') => self.open_settings(),
            KeyCode::Char(c @ '1'..='4') => {
                if let Some(view) = SmartView::from_shortcut(c) {
                    self.set_smart_view(view);
                    self.toast(&format!("view: {}", view.label()));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_key_pending_delete(&mut self, code: KeyCode) -> TuiResult<()> {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.execute_delete()?;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.pending_delete = None;
                self.toast("delete cancelled");
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.confirm_delete_yes = true;
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.confirm_delete_yes = false;
            }
            KeyCode::Enter => {
                if self.confirm_delete_yes {
                    self.execute_delete()?;
                } else {
                    self.pending_delete = None;
                    self.toast("delete cancelled");
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn on_key_task_card(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                self.show_task_card = false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.task_card_scroll = self.task_card_scroll.saturating_add(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.task_card_scroll = self.task_card_scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.task_card_scroll = self.task_card_scroll.saturating_add(10);
            }
            KeyCode::PageUp => {
                self.task_card_scroll = self.task_card_scroll.saturating_sub(10);
            }
            _ => {}
        }
    }

    /// When focus is on the Log panel, capture pager-style navigation keys
    /// (j/k, PgUp/PgDn, Ctrl-d/u, g/G, Home/End, /, n/N, F). Returns true when
    /// the key was consumed so the main handler can skip it.
    fn handle_log_pager_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.log_view.scroll_down(1);
                true
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.log_view.scroll_up(1);
                true
            }
            KeyCode::Char('d') if ctrl => {
                let n = self.log_view.half_page();
                self.log_view.scroll_down(n);
                true
            }
            KeyCode::Char('u') if ctrl => {
                let n = self.log_view.half_page();
                self.log_view.scroll_up(n);
                true
            }
            KeyCode::PageDown => {
                let n = self.log_view.page();
                self.log_view.scroll_down(n);
                true
            }
            KeyCode::PageUp => {
                let n = self.log_view.page();
                self.log_view.scroll_up(n);
                true
            }
            KeyCode::Home => {
                self.log_view.scroll_to_top();
                true
            }
            KeyCode::End => {
                self.log_view.goto_bottom();
                true
            }
            KeyCode::Char('g') => {
                self.log_view.scroll_to_top();
                true
            }
            KeyCode::Char('G') => {
                self.log_view.goto_bottom();
                true
            }
            KeyCode::Char('/') => {
                self.enter_input(InputMode::LogSearch, "Log search: ");
                true
            }
            KeyCode::Char('n') => {
                self.log_view.next_match();
                true
            }
            KeyCode::Char('N') => {
                self.log_view.prev_match();
                true
            }
            KeyCode::Char('F') => {
                self.log_view.toggle_follow();
                let msg = if self.log_view.follow {
                    "log: follow on"
                } else {
                    "log: follow off"
                };
                self.toast(msg);
                true
            }
            _ => false,
        }
    }

    fn on_key_chord(&mut self, code: KeyCode, _mods: KeyModifiers) -> TuiResult<()> {
        if matches!(code, KeyCode::Esc) {
            self.end_chord();
            return Ok(());
        }
        match self.pending_chord {
            PendingChord::CtrlW | PendingChord::LeaderWindow => {
                self.end_chord();
                if let Some(dir) = direction_from_key(code) {
                    self.apply_window_move(dir);
                }
                Ok(())
            }
            PendingChord::Leader => self.on_key_leader(code),
            PendingChord::LeaderTask => {
                self.end_chord();
                self.on_key_leader_task(code)
            }
            PendingChord::LeaderView => {
                self.end_chord();
                self.on_key_leader_view(code);
                Ok(())
            }
            PendingChord::None => Ok(()),
        }
    }

    fn on_key_leader(&mut self, code: KeyCode) -> TuiResult<()> {
        match code {
            KeyCode::Char('w') => {
                self.begin_chord(PendingChord::LeaderWindow);
            }
            KeyCode::Char('t') => {
                self.begin_chord(PendingChord::LeaderTask);
            }
            KeyCode::Char('v') => {
                self.begin_chord(PendingChord::LeaderView);
            }
            KeyCode::Char('/') | KeyCode::Char('f') => {
                self.end_chord();
                self.enter_input(InputMode::Filter, "Fuzzy: ");
            }
            KeyCode::Char('a') => {
                self.end_chord();
                self.open_create_task_form();
            }
            KeyCode::Char('A') => {
                self.end_chord();
                self.enter_input(InputMode::AddTask, "Quick add task: ");
            }
            KeyCode::Char('T') => {
                self.end_chord();
                self.cycle_theme()?;
            }
            KeyCode::Char('S') | KeyCode::Char(',') => {
                self.end_chord();
                self.open_settings();
            }
            KeyCode::Char('q') => {
                self.end_chord();
                self.running = false;
            }
            _ => self.end_chord(),
        }
        Ok(())
    }

    fn on_key_leader_task(&mut self, code: KeyCode) -> TuiResult<()> {
        match code {
            KeyCode::Char('d') => self.action_done(),
            KeyCode::Char('x') => {
                self.action_delete();
                Ok(())
            }
            KeyCode::Char('e') => {
                self.open_edit_task_form();
                Ok(())
            }
            KeyCode::Char('i') => self.action_impl(),
            KeyCode::Char('s') | KeyCode::Char('y') => self.action_spawn(),
            KeyCode::Char('K') => self.action_kill(),
            KeyCode::Char('r') => self.action_review(),
            KeyCode::Char('R') => {
                self.enter_input(InputMode::Reject, "Reject reason: ");
                Ok(())
            }
            KeyCode::Char('m') => self.action_merge(),
            _ => Ok(()),
        }
    }

    fn on_key_leader_view(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('g') => self.toggle_dag(),
            KeyCode::Char('l') => self.toggle_log(),
            KeyCode::Char('/') => self.enter_input(InputMode::Filter, "Fuzzy: "),
            KeyCode::Char('T') => {
                if let Err(e) = self.cycle_theme() {
                    self.toast(&format!("theme: {e}"));
                }
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
            }
            KeyCode::Char(c @ ('1' | '2' | '3' | '4')) => {
                if let Some(view) = SmartView::from_shortcut(c) {
                    self.set_smart_view(view);
                    self.toast(&format!("view: {}", view.label()));
                }
            }
            KeyCode::Char('t') => {
                self.set_smart_view(SmartView::Today);
                self.toast("view: Today");
            }
            KeyCode::Char('u') => {
                self.set_smart_view(SmartView::Upcoming);
                self.toast("view: Upcoming");
            }
            KeyCode::Char('i') => {
                self.set_smart_view(SmartView::Inbox);
                self.toast("view: Inbox");
            }
            KeyCode::Char('a') => {
                self.set_smart_view(SmartView::All);
                self.toast("view: All");
            }
            _ => {}
        }
    }

    fn on_key_input(&mut self, code: KeyCode, mods: KeyModifiers) -> TuiResult<()> {
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let alt = mods.contains(KeyModifiers::ALT);
        let shift = mods.contains(KeyModifiers::SHIFT);
        let multiline_ok = matches!(self.input_mode, InputMode::AddTask | InputMode::Reject);

        match code {
            KeyCode::Esc => {
                self.cancel_input();
                return Ok(());
            }
            KeyCode::Enter => {
                let wants_newline =
                    multiline_ok && (alt || shift || self.input.ends_with_backslash());
                if wants_newline {
                    if self.input.ends_with_backslash() {
                        self.input.pop_trailing_backslash();
                    }
                    self.input.insert_newline();
                    self.sync_live_input();
                } else {
                    self.commit_input()?;
                }
                return Ok(());
            }
            KeyCode::Backspace => {
                if ctrl || alt {
                    self.input.delete_word_back();
                } else {
                    self.input.backspace();
                }
            }
            KeyCode::Delete => {
                self.input.delete_forward();
            }
            KeyCode::Left => {
                if ctrl || alt {
                    self.input.move_word_back();
                } else {
                    self.input.move_left();
                }
            }
            KeyCode::Right => {
                if ctrl || alt {
                    self.input.move_word_forward();
                } else {
                    self.input.move_right();
                }
            }
            KeyCode::Up if multiline_ok => self.input.move_up(),
            KeyCode::Down if multiline_ok => self.input.move_down(),
            KeyCode::Home => self.input.move_home(),
            KeyCode::End => self.input.move_end(),
            KeyCode::PageUp if multiline_ok => self.input.move_doc_start(),
            KeyCode::PageDown if multiline_ok => self.input.move_doc_end(),
            KeyCode::Char(c) if ctrl => match c {
                'a' => self.input.move_home(),
                'e' => self.input.move_end(),
                'b' => self.input.move_left(),
                'f' => self.input.move_right(),
                'p' if multiline_ok => self.input.move_up(),
                'n' if multiline_ok => self.input.move_down(),
                'h' => self.input.backspace(),
                'd' => self.input.delete_forward(),
                'w' => self.input.delete_word_back(),
                'k' => self.input.kill_to_end(),
                'u' => self.input.kill_to_start(),
                'j' if multiline_ok => self.input.insert_newline(),
                'v' => self.paste_clipboard(),
                'g' => {
                    self.cancel_input();
                    return Ok(());
                }
                _ => {}
            },
            KeyCode::Char(c) if alt => match c {
                'b' => self.input.move_word_back(),
                'f' => self.input.move_word_forward(),
                'd' => self.input.kill_word_forward(),
                _ => {}
            },
            KeyCode::Char(c) => {
                self.input.insert_char(c);
            }
            _ => {}
        }
        self.sync_live_input();
        Ok(())
    }

    fn commit_input(&mut self) -> TuiResult<()> {
        let mode = self.input_mode;
        let buf = match mode {
            InputMode::Filter | InputMode::LogSearch => self.input.text_single_line(),
            _ => self.input.text(),
        };
        self.input.clear();
        self.input_mode = InputMode::Normal;
        match mode {
            InputMode::AddTask => {
                let title = buf.trim();
                if title.is_empty() {
                    self.toast("add: empty title, cancelled");
                    return Ok(());
                }
                let epic = if self.current_epic() == ALL_EPIC {
                    "default".to_string()
                } else {
                    self.current_epic().to_string()
                };
                let id = self.store.next_task_id(&self.tasks);
                let task = Task {
                    id: id.clone(),
                    title: title.to_string(),
                    epic,
                    status: StatusId("todo".into()),
                    priority: tc_core::task::Priority::default(),
                    tags: vec![],
                    due: None,
                    scheduled: None,
                    estimate: None,
                    depends_on: vec![],
                    files: vec![],
                    pack_exclude: vec![],
                    notes: String::new(),
                    acceptance_criteria: vec![],
                    assignee: None,
                    created_at: chrono::Utc::now(),
                };
                let mut new_tasks = self.tasks.clone();
                new_tasks.push(task);
                self.store.save_tasks(&new_tasks)?;
                self.reload_tasks()?;
                self.clear_draft();
                self.toast(&format!("added {}", id.0));
            }
            InputMode::Filter => {
                self.filter = buf;
                self.selected_task = 0;
            }
            InputMode::Reject => {
                let feedback = buf.trim().to_string();
                if feedback.is_empty() {
                    self.toast("reject: empty feedback, cancelled");
                    return Ok(());
                }
                self.action_reject(&feedback)?;
            }
            InputMode::LogSearch => {
                self.log_view.set_search(buf);
            }
            InputMode::Normal => {}
        }
        Ok(())
    }

    pub(super) fn on_click(&mut self, col: u16, row: u16) -> TuiResult<()> {
        if self.screen != AppScreen::CreateTask {
            return Ok(());
        }
        let Some(form) = &mut self.create_task_form else {
            return Ok(());
        };
        let size = crossterm::terminal::size().unwrap_or((80, 24));
        let frame_area = ratatui::layout::Rect::new(0, 0, size.0, size.1);
        let areas = crate::components::create_task_form::compute_field_areas(frame_area, form);
        for (field, rect) in &areas {
            if col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
            {
                form.active_field = *field;
                return Ok(());
            }
        }
        Ok(())
    }

    fn on_key_create_task(&mut self, code: KeyCode, mods: KeyModifiers) -> TuiResult<()> {
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let shift = mods.contains(KeyModifiers::SHIFT);
        let alt = mods.contains(KeyModifiers::ALT);

        let Some(mut form) = self.create_task_form.take() else {
            self.screen = AppScreen::Main;
            return Ok(());
        };

        match code {
            KeyCode::Esc => {
                self.screen = AppScreen::Main;
                self.toast("create task cancelled");
                return Ok(());
            }
            KeyCode::Char('g') if ctrl => {
                self.screen = AppScreen::Main;
                self.toast("create task cancelled");
                return Ok(());
            }
            KeyCode::Char('s') if ctrl => {
                self.create_task_form = Some(form);
                return self.submit_create_task();
            }
            KeyCode::BackTab => {
                form.prev_field();
            }
            KeyCode::Tab => {
                if shift {
                    form.prev_field();
                } else {
                    form.next_field();
                }
            }
            _ => dispatch_create_task_field_key(&mut form, code, ctrl, alt, shift),
        }

        self.create_task_form = Some(form);
        Ok(())
    }
}

// Crossterm KeyEvent arrives with (code, modifiers); we route each field's
// keys through its own matcher to keep `on_key_create_task` readable.
fn dispatch_create_task_field_key(
    form: &mut CreateTaskForm,
    code: KeyCode,
    ctrl: bool,
    alt: bool,
    shift: bool,
) {
    let field = form.active_field;
    match field {
        CreateTaskField::Priority => match code {
            KeyCode::Left | KeyCode::Char('h') if !ctrl && !alt => {
                form.cycle_priority_prev();
            }
            KeyCode::Right | KeyCode::Char('l') if !ctrl && !alt => {
                form.cycle_priority_next();
            }
            KeyCode::Enter => form.next_field(),
            _ => {}
        },
        CreateTaskField::Assignee => match code {
            KeyCode::Left | KeyCode::Char('h') if !ctrl && !alt => {
                form.cycle_assignee_prev();
            }
            KeyCode::Right | KeyCode::Char('l') if !ctrl && !alt => {
                form.cycle_assignee_next();
            }
            KeyCode::Enter => form.next_field(),
            _ => {}
        },
        CreateTaskField::DependsOn => {
            let empty = form.dep_input.is_empty();
            match code {
                KeyCode::Enter => {
                    if !form.try_commit_dep() {
                        form.next_field();
                    }
                }
                KeyCode::Backspace if empty => {
                    form.depends_on.pop();
                }
                _ => apply_editor_key(&mut form.dep_input, code, ctrl, alt, false),
            }
        }
        CreateTaskField::AcceptanceCriteria => {
            let empty = form.ac_input.is_empty();
            match code {
                KeyCode::Enter => {
                    if !form.try_commit_ac() {
                        form.next_field();
                    }
                }
                KeyCode::Backspace if empty => {
                    form.acceptance_criteria.pop();
                }
                _ => apply_editor_key(&mut form.ac_input, code, ctrl, alt, false),
            }
        }
        CreateTaskField::Files => {
            let empty = form.files_input.is_empty();
            match code {
                KeyCode::Enter => {
                    if !form.try_commit_file() {
                        form.next_field();
                    }
                }
                KeyCode::Backspace if empty => {
                    form.files.pop();
                }
                _ => apply_editor_key(&mut form.files_input, code, ctrl, alt, false),
            }
        }
        CreateTaskField::Title => match code {
            KeyCode::Enter => {
                if alt || shift || form.title.ends_with_backslash() {
                    if form.title.ends_with_backslash() {
                        form.title.pop_trailing_backslash();
                    }
                    form.title.insert_newline();
                } else {
                    form.next_field();
                }
            }
            _ => apply_editor_key(&mut form.title, code, ctrl, alt, true),
        },
        CreateTaskField::Epic => match code {
            KeyCode::Enter => form.next_field(),
            _ => apply_editor_key(&mut form.epic, code, ctrl, alt, false),
        },
        CreateTaskField::Notes => match code {
            KeyCode::Enter => {
                if alt || shift || form.notes.ends_with_backslash() {
                    if form.notes.ends_with_backslash() {
                        form.notes.pop_trailing_backslash();
                    }
                    form.notes.insert_newline();
                } else {
                    form.next_field();
                }
            }
            _ => apply_editor_key(&mut form.notes, code, ctrl, alt, true),
        },
    }
}

fn apply_editor_key(editor: &mut Editor, code: KeyCode, ctrl: bool, alt: bool, multiline: bool) {
    match code {
        KeyCode::Backspace => {
            if ctrl || alt {
                editor.delete_word_back();
            } else {
                editor.backspace();
            }
        }
        KeyCode::Delete => editor.delete_forward(),
        KeyCode::Left => {
            if ctrl || alt {
                editor.move_word_back();
            } else {
                editor.move_left();
            }
        }
        KeyCode::Right => {
            if ctrl || alt {
                editor.move_word_forward();
            } else {
                editor.move_right();
            }
        }
        KeyCode::Up if multiline => editor.move_up(),
        KeyCode::Down if multiline => editor.move_down(),
        KeyCode::Home => editor.move_home(),
        KeyCode::End => editor.move_end(),
        KeyCode::Char(c) if ctrl => match c {
            'a' => editor.move_home(),
            'e' => editor.move_end(),
            'b' => editor.move_left(),
            'f' => editor.move_right(),
            'h' => editor.backspace(),
            'd' => editor.delete_forward(),
            'w' => editor.delete_word_back(),
            'k' => editor.kill_to_end(),
            'u' => editor.kill_to_start(),
            'j' if multiline => editor.insert_newline(),
            'p' if multiline => editor.move_up(),
            'n' if multiline => editor.move_down(),
            _ => {}
        },
        KeyCode::Char(c) if alt => match c {
            'b' => editor.move_word_back(),
            'f' => editor.move_word_forward(),
            'd' => editor.kill_word_forward(),
            _ => {}
        },
        KeyCode::Char(c) => editor.insert_char(c),
        _ => {}
    }
}

fn direction_from_key(code: KeyCode) -> Option<Direction> {
    match code {
        KeyCode::Char('h') | KeyCode::Left => Some(Direction::Left),
        KeyCode::Char('j') | KeyCode::Down => Some(Direction::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(Direction::Up),
        KeyCode::Char('l') | KeyCode::Right => Some(Direction::Right),
        _ => None,
    }
}
