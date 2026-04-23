use std::collections::BTreeMap;
use std::time::Instant;

use tc_core::config::TcConfig;
use tc_core::dag::TaskDag;
use tc_core::status::StatusMachine;
use tc_core::task::{Task, TaskId};
use tc_spawn::process::WorkerState;
use tc_storage::Store;

use crate::create_task::CreateTaskForm;
use crate::editor::Editor;
use crate::keybind::PendingChord;
use crate::log_view::LogView;

use super::settings::SettingsState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    Epics,
    Tasks,
    Log,
    Detail,
    Dag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Main,
    CreateTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    AddTask,
    Filter,
    Reject,
    LogSearch,
}

/// Actions that require the runtime to suspend or alter the TUI.
#[derive(Debug, Clone)]
pub enum TuiAction {
    None,
    /// Suspend TUI, run interactive claude for the given task, then resume.
    SuspendForImpl(TaskId),
    /// Suspend TUI, show diff in $PAGER, then resume.
    SuspendForReview(TaskId),
}

/// Application state for the TUI.
pub struct App {
    pub store: Store,
    pub config: TcConfig,
    pub tasks: Vec<Task>,
    pub dag: TaskDag,
    pub status_machine: StatusMachine,

    pub epics: Vec<String>,
    pub epic_counts: BTreeMap<String, usize>,
    pub selected_epic: usize,

    pub selected_task: usize,
    pub focus: FocusPanel,

    pub show_dag: bool,
    pub show_log: bool,
    pub show_help: bool,
    pub show_task_card: bool,
    pub task_card_scroll: u16,
    pub pending_delete: Option<Task>,
    pub confirm_delete_yes: bool,

    pub workers: Vec<WorkerState>,
    pub max_workers: usize,

    pub input_mode: InputMode,
    pub input: Editor,
    pub filter: String,

    pub status_message: String,
    pub log_view: LogView,

    pub pending_chord: PendingChord,
    pub chord_started_at: Option<Instant>,

    pub pending_action: TuiAction,
    pub running: bool,

    pub screen: AppScreen,
    pub create_task_form: Option<CreateTaskForm>,
    pub settings: Option<SettingsState>,
}
