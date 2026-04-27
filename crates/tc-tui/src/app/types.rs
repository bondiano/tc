use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use tc_core::config::TcConfig;
use tc_core::dag::TaskDag;
use tc_core::status::StatusMachine;
use tc_core::task::{Task, TaskId};
use tc_spawn::process::WorkerState;
use tc_storage::Store;

/// Predefined "smart view" tabs surfaced in the header (M-7.2).
///
/// `Today`/`Upcoming`/`Inbox` mirror the CLI smart-view filters from
/// `tc_core::filter` semantics so the TUI shortcuts feel identical to
/// `tc today` / `tc upcoming` / `tc inbox`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmartView {
    #[default]
    All,
    Today,
    Upcoming,
    Inbox,
}

impl SmartView {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Today => "Today",
            Self::Upcoming => "Upcoming",
            Self::Inbox => "Inbox",
        }
    }

    pub fn shortcut(self) -> char {
        match self {
            Self::Today => '1',
            Self::Upcoming => '2',
            Self::Inbox => '3',
            Self::All => '4',
        }
    }

    pub fn all() -> [SmartView; 4] {
        [Self::Today, Self::Upcoming, Self::Inbox, Self::All]
    }

    pub fn from_shortcut(c: char) -> Option<Self> {
        match c {
            '1' => Some(Self::Today),
            '2' => Some(Self::Upcoming),
            '3' => Some(Self::Inbox),
            '4' => Some(Self::All),
            _ => None,
        }
    }
}

use crate::create_task::CreateTaskForm;
use crate::editor::Editor;
use crate::keybind::PendingChord;
use crate::log_view::LogView;
use crate::theme::Palette;

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
    /// Active fuzzy/filter query. Applied as a fuzzy match (M-7.1) on top of
    /// the current epic + smart view selection.
    pub filter: String,
    pub smart_view: SmartView,

    pub status_message: String,
    pub log_view: LogView,

    pub pending_chord: PendingChord,
    pub chord_started_at: Option<Instant>,

    pub pending_action: TuiAction,
    pub running: bool,

    pub screen: AppScreen,
    pub create_task_form: Option<CreateTaskForm>,
    pub settings: Option<SettingsState>,
    pub palette: Palette,

    /// Map of task IDs that just transitioned to a terminal status, to the
    /// instant the transition fired. Renderer fades + strikes through these
    /// rows for [`COMPLETION_ANIMATION_DURATION`] (M-7.8).
    pub completion_animations: HashMap<TaskId, Instant>,
}
