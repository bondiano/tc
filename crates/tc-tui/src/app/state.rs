use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDate};
use tc_core::dag::TaskDag;
use tc_core::fuzzy;
use tc_core::status::StatusMachine;
use tc_core::task::{Task, TaskId};
use tc_spawn::process::{WorkerState, WorkerStatus};
use tc_spawn::scheduler::list_worker_states;
use tc_storage::Store;

use crate::editor::Editor;
use crate::error::TuiResult;
use crate::keybind::PendingChord;
use crate::log_view::{LogView, MAX_LOG_LINES};
use crate::theme::{self, Palette};

use super::ALL_EPIC;
use super::types::{App, AppScreen, FocusPanel, InputMode, SmartView, TuiAction};

/// Number of days included in the `Upcoming` smart view (exclusive of today).
/// Mirrors the default for `tc upcoming` to keep CLI/TUI semantics aligned.
pub(super) const UPCOMING_HORIZON_DAYS: u32 = 7;

/// How long the strikethrough + fade plays after a task hits a terminal
/// status (M-7.8). Long enough that the user notices, short enough that
/// the row settles to its final styling before they reach for the next
/// action.
pub const COMPLETION_ANIMATION_DURATION: Duration = Duration::from_millis(900);

impl App {
    pub fn new(store: Store) -> TuiResult<Self> {
        let config = store.load_config()?;
        let tasks = store.load_tasks()?;
        let dag = TaskDag::from_tasks(&tasks)?;
        let status_machine = StatusMachine::new(config.statuses.clone());
        let max_workers = config.spawn.max_parallel;
        let palette = Palette::from_theme(&theme::resolve(&config.ui.theme));

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
            smart_view: SmartView::default(),
            status_message: String::from("Press ? for help, q to quit"),
            log_view: LogView::new(),
            pending_chord: PendingChord::None,
            chord_started_at: None,
            pending_action: TuiAction::None,
            running: true,
            screen: AppScreen::Main,
            create_task_form: None,
            settings: None,
            palette,
            completion_animations: HashMap::new(),
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
        let today = Local::now().date_naive();

        let pre_filtered: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| epic == ALL_EPIC || t.epic == epic)
            .filter(|t| smart_view_matches(self.smart_view, t, today, &self.status_machine))
            .collect();

        let query = self.filter.trim();
        if query.is_empty() {
            return pre_filtered;
        }

        // Fuzzy rank against pre-filtered slice (M-7.1). We owned-clone to
        // call `tc_core::fuzzy::search`, then map back to references so the
        // caller's contract (Vec<&Task>) is unchanged.
        let owned: Vec<Task> = pre_filtered.iter().map(|t| (*t).clone()).collect();
        let hits = fuzzy::search(&owned, query, owned.len());
        let by_id: HashMap<&str, &Task> =
            pre_filtered.iter().map(|t| (t.id.0.as_str(), *t)).collect();
        hits.into_iter()
            .filter_map(|h| by_id.get(h.id.0.as_str()).copied())
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
        let lines = tc_executor::log_tail::read_tail_lines(&path, MAX_LOG_LINES);
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

    /// Switch the active smart view (M-7.2). Resets selection to the top of
    /// the new view -- the previously selected row is rarely meaningful in a
    /// different list.
    pub fn set_smart_view(&mut self, view: SmartView) {
        if self.smart_view == view {
            return;
        }
        self.smart_view = view;
        self.selected_task = 0;
    }

    /// Advance to the next built-in theme preset (M-7.9). Persists the new
    /// name to `.tc/config.yaml` so the choice survives a restart, but the
    /// in-memory palette swaps in immediately so the user sees the change
    /// on the very next frame.
    pub fn cycle_theme(&mut self) -> TuiResult<()> {
        let presets = tc_core::theme::Theme::PRESET_NAMES;
        let current = self.config.ui.theme.as_str();
        let idx = presets
            .iter()
            .position(|n| n.eq_ignore_ascii_case(current))
            .unwrap_or(0);
        let next = presets[(idx + 1) % presets.len()];
        self.config.ui.theme = next.to_string();
        self.palette = crate::theme::Palette::from_theme(&crate::theme::resolve(next));
        self.store.save_config(&self.config)?;
        self.toast(&format!("theme: {next}"));
        Ok(())
    }

    /// Record that a task has just transitioned to a terminal status so the
    /// renderer can play the completion animation (M-7.8).
    pub fn mark_completed(&mut self, id: &TaskId) {
        self.completion_animations
            .insert(id.clone(), Instant::now());
    }

    /// Drop completion-animation entries that have run their course. Called
    /// from the runtime tick so an old `done` event never lingers as a
    /// permanent strikethrough on a row.
    pub fn prune_completion_animations(&mut self) {
        self.completion_animations
            .retain(|_, started| started.elapsed() < COMPLETION_ANIMATION_DURATION);
    }

    /// Animation progress in `[0.0, 1.0]` for a task, or `None` if the task
    /// has no live animation. `0.0` is the moment of completion;
    /// `1.0` means the animation just finished and the row should render in
    /// its final state.
    pub fn completion_progress(&self, id: &TaskId) -> Option<f32> {
        let started = self.completion_animations.get(id)?;
        let elapsed = started.elapsed();
        if elapsed >= COMPLETION_ANIMATION_DURATION {
            return None;
        }
        let total = COMPLETION_ANIMATION_DURATION.as_secs_f32();
        Some((elapsed.as_secs_f32() / total).clamp(0.0, 1.0))
    }

    /// Wake the runtime sooner if any completion animation is still
    /// playing, so the fade doesn't hitch waiting for the next event tick.
    pub fn animation_wake_in(&self) -> Option<Duration> {
        self.completion_animations
            .values()
            .filter_map(|started| {
                let e = started.elapsed();
                if e >= COMPLETION_ANIMATION_DURATION {
                    None
                } else {
                    Some(COMPLETION_ANIMATION_DURATION - e)
                }
            })
            .min()
    }
}

/// Mirror the predicates used by `tc today` / `tc upcoming` / `tc inbox`. We
/// duplicate the small predicate logic here (rather than depending on the
/// `tc` binary) because tc-tui sits below `tc` in the crate graph.
pub(super) fn smart_view_matches(
    view: SmartView,
    task: &Task,
    today: NaiveDate,
    sm: &StatusMachine,
) -> bool {
    match view {
        SmartView::All => true,
        SmartView::Today => {
            !sm.is_terminal(&task.status)
                && (task.due == Some(today) || task.scheduled == Some(today))
        }
        SmartView::Upcoming => {
            if sm.is_terminal(&task.status) {
                return false;
            }
            let horizon = today
                .checked_add_days(chrono::Days::new(UPCOMING_HORIZON_DAYS as u64))
                .unwrap_or(today);
            let in_window = |d: Option<NaiveDate>| d.is_some_and(|v| v > today && v <= horizon);
            in_window(task.due) || in_window(task.scheduled)
        }
        SmartView::Inbox => {
            !sm.is_terminal(&task.status) && task.due.is_none() && task.scheduled.is_none()
        }
    }
}
