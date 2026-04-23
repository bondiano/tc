use std::collections::BTreeMap;

use tc_core::dag::TaskDag;
use tc_core::status::StatusMachine;
use tc_core::task::{Task, TaskId};
use tc_spawn::process::{WorkerState, WorkerStatus};
use tc_spawn::scheduler::list_worker_states;
use tc_storage::Store;

use crate::editor::Editor;
use crate::error::TuiResult;
use crate::keybind::PendingChord;
use crate::log_view::{LogView, MAX_LOG_LINES};

use super::ALL_EPIC;
use super::types::{App, AppScreen, FocusPanel, InputMode, TuiAction};

impl App {
    pub fn new(store: Store) -> TuiResult<Self> {
        let config = store.load_config()?;
        let tasks = store.load_tasks()?;
        let dag = TaskDag::from_tasks(&tasks)?;
        let status_machine = StatusMachine::new(config.statuses.clone());
        let max_workers = config.spawn.max_parallel;

        let mut app = Self {
            store,
            config,
            tasks,
            dag,
            status_machine,
            epics: Vec::new(),
            epic_counts: BTreeMap::new(),
            selected_epic: 0,
            selected_task: 0,
            focus: FocusPanel::Tasks,
            show_dag: false,
            show_log: false,
            show_help: false,
            show_task_card: false,
            task_card_scroll: 0,
            pending_delete: None,
            confirm_delete_yes: false,
            workers: Vec::new(),
            max_workers,
            input_mode: InputMode::Normal,
            input: Editor::new(),
            filter: String::new(),
            status_message: String::from("Press ? for help, q to quit"),
            log_view: LogView::new(),
            pending_chord: PendingChord::None,
            chord_started_at: None,
            pending_action: TuiAction::None,
            running: true,
            screen: AppScreen::Main,
            create_task_form: None,
            settings: None,
        };
        app.recompute_epics();
        app.refresh_workers();
        Ok(app)
    }

    pub(super) fn recompute_epics(&mut self) {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for t in &self.tasks {
            *counts.entry(t.epic.clone()).or_insert(0) += 1;
        }
        let mut epics = vec![ALL_EPIC.to_string()];
        epics.extend(counts.keys().cloned());
        self.epic_counts = counts;
        self.epics = epics;
        if self.selected_epic >= self.epics.len() {
            self.selected_epic = 0;
        }
    }

    pub fn current_epic(&self) -> &str {
        self.epics
            .get(self.selected_epic)
            .map(String::as_str)
            .unwrap_or(ALL_EPIC)
    }

    pub fn epic_count(&self, epic: &str) -> usize {
        if epic == ALL_EPIC {
            return self.tasks.len();
        }
        self.epic_counts.get(epic).copied().unwrap_or(0)
    }

    pub fn visible_tasks(&self) -> Vec<&Task> {
        let epic = self.current_epic().to_string();
        let needle = self.filter.to_lowercase();
        self.tasks
            .iter()
            .filter(|t| epic == ALL_EPIC || t.epic == epic)
            .filter(|t| {
                if needle.is_empty() {
                    return true;
                }
                t.id.0.to_lowercase().contains(&needle) || t.title.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn selected_task(&self) -> Option<Task> {
        self.visible_tasks()
            .get(self.selected_task)
            .map(|t| (*t).clone())
    }

    pub fn worker_for(&self, id: &TaskId) -> Option<&WorkerState> {
        self.workers.iter().find(|w| w.task_id == id.0)
    }

    pub fn refresh_workers(&mut self) {
        let dir = self.store.workers_dir();
        if let Ok(states) = list_worker_states(&dir) {
            self.workers = states;
        }
    }

    pub fn reload_tasks(&mut self) -> TuiResult<()> {
        let tasks = self.store.load_tasks()?;
        let dag = TaskDag::from_tasks(&tasks)?;
        self.tasks = tasks;
        self.dag = dag;
        self.recompute_epics();
        let visible = self.visible_tasks().len();
        if visible == 0 {
            self.selected_task = 0;
        } else if self.selected_task >= visible {
            self.selected_task = visible - 1;
        }
        Ok(())
    }

    pub fn tail_log(&mut self) {
        let Some(t) = self.selected_task() else {
            self.log_view.reset();
            self.log_view.task = None;
            return;
        };
        self.log_view.sync_task(Some(&t.id));
        let path = self.store.log_path(&t.id);
        let lines = match std::fs::read_to_string(&path) {
            Ok(content) => {
                let mut tail: Vec<String> = content
                    .lines()
                    .rev()
                    .take(MAX_LOG_LINES)
                    .map(String::from)
                    .collect();
                tail.reverse();
                tail
            }
            Err(_) => vec![format!("(no log at {})", path.display())],
        };
        self.log_view.set_lines(lines);
    }

    pub fn toast(&mut self, msg: &str) {
        self.status_message = msg.to_string();
    }

    pub fn workers_summary(&self) -> String {
        let active = self
            .workers
            .iter()
            .filter(|w| matches!(w.status, WorkerStatus::Running))
            .count();
        format!("Running: {}/{}", active, self.max_workers)
    }

    /// Take the pending action, replacing it with None.
    pub fn take_action(&mut self) -> TuiAction {
        std::mem::replace(&mut self.pending_action, TuiAction::None)
    }
}
