use tc_core::dag::TaskDag;
use tc_spawn::process::WorkerStatus;
use tc_spawn::recovery::{WorkerTransition, reconcile_workers};

use crate::create_task::CreateTaskField;
use crate::error::TuiResult;
use crate::event::Message;

use super::types::{App, AppScreen, InputMode};

impl App {
    /// Apply a Message to the App; returns Ok(()) on success.
    pub fn update(&mut self, msg: Message) -> TuiResult<()> {
        match msg {
            Message::Quit => {
                if self.input_mode == InputMode::AddTask && !self.input.is_empty() {
                    self.save_draft();
                }
                self.running = false;
                Ok(())
            }
            Message::Paste(ref text) if self.screen == AppScreen::CreateTask => {
                if let Some(form) = &mut self.create_task_form {
                    match form.active_field {
                        CreateTaskField::Title => form.title.insert_str(text),
                        CreateTaskField::Epic => form.epic.insert_str(text),
                        CreateTaskField::DependsOn => {
                            let joined = text.replace('\n', " ");
                            form.dep_input.insert_str(&joined);
                        }
                        CreateTaskField::Notes => form.notes.insert_str(text),
                        CreateTaskField::AcceptanceCriteria => {
                            let joined = text.replace('\n', " ");
                            form.ac_input.insert_str(&joined);
                        }
                        CreateTaskField::Files => {
                            let joined = text.replace('\n', " ");
                            form.files_input.insert_str(&joined);
                        }
                        CreateTaskField::Priority | CreateTaskField::Assignee => {}
                    }
                }
                Ok(())
            }
            Message::Tick => self.on_tick(),
            Message::Key(code, mods) => self.on_key(code, mods),
            Message::Click { col, row } => self.on_click(col, row),
            Message::Paste(text) => {
                self.on_paste(&text);
                Ok(())
            }
        }
    }

    /// Insert pasted text into the active input editor, if any. Paste events
    /// arriving while no input mode is open are dropped silently.
    pub fn on_paste(&mut self, text: &str) {
        if self.input_mode == InputMode::Normal {
            return;
        }
        if matches!(self.input_mode, InputMode::Filter | InputMode::LogSearch) {
            let joined = text.replace('\n', " ");
            self.input.insert_str(&joined);
        } else {
            self.input.insert_str(text);
        }
        self.sync_live_input();
    }

    /// Push the editor's current text into downstream state for modes that
    /// update live (Filter, LogSearch).
    pub(super) fn sync_live_input(&mut self) {
        match self.input_mode {
            InputMode::Filter => {
                self.filter = self.input.text_single_line();
            }
            InputMode::LogSearch => {
                self.log_view.set_search(self.input.text_single_line());
            }
            _ => {}
        }
    }

    pub(super) fn on_tick(&mut self) -> TuiResult<()> {
        self.reconcile_and_toast();
        if self.show_log {
            self.tail_log();
        }
        Ok(())
    }

    /// Reconcile worker state files and emit a toast for any transitions.
    /// Called on every tick so tasks spawned from the TUI advance to
    /// `review`/`blocked` as soon as the underlying tmux session exits.
    /// Also updates `self.workers` from the returned states to avoid a second scan.
    fn reconcile_and_toast(&mut self) {
        let Ok((transitions, states)) = reconcile_workers(&self.store) else {
            return;
        };
        self.workers = states;
        if transitions.is_empty() {
            return;
        }
        if let Ok(tasks) = self.store.load_tasks() {
            self.tasks = tasks;
            if let Ok(dag) = TaskDag::from_tasks(&self.tasks) {
                self.dag = dag;
            }
            self.recompute_epics();
        }
        self.toast(&summarize_transitions(&transitions));
    }
}

fn summarize_transitions(ts: &[WorkerTransition]) -> String {
    use std::fmt::Write as _;
    let mut parts: Vec<String> = Vec::with_capacity(ts.len());
    for t in ts {
        let label = match t.status {
            WorkerStatus::Completed => "done",
            WorkerStatus::Failed => "failed",
            WorkerStatus::Killed => "killed",
            WorkerStatus::Running => continue,
        };
        let mut entry = format!("{} {label}", t.task_id);
        if let Some(code) = t.exit_code
            && code != 0
        {
            let _ = write!(entry, " (exit {code})");
        }
        parts.push(entry);
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("worker: {}", parts.join(", "))
}
