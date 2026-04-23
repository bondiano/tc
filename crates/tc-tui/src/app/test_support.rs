use std::collections::BTreeMap;

use chrono::Utc;
use tc_core::config::{
    ExecutorConfig, PackerConfig, ResolverConfig, SandboxConfig, SpawnConfig, TcConfig,
    VerificationConfig,
};
use tc_core::dag::TaskDag;
use tc_core::status::{StatusDef, StatusId, StatusMachine};
use tc_core::task::{Task, TaskId};
use tc_storage::Store;

use crate::editor::Editor;
use crate::keybind::PendingChord;
use crate::log_view::LogView;

use super::types::{App, AppScreen, FocusPanel, InputMode, TuiAction};

pub fn dummy_config() -> TcConfig {
    TcConfig {
        statuses: vec![
            StatusDef {
                id: StatusId("todo".into()),
                label: "Todo".into(),
                terminal: false,
            },
            StatusDef {
                id: StatusId("in_progress".into()),
                label: "In Progress".into(),
                terminal: false,
            },
            StatusDef {
                id: StatusId("done".into()),
                label: "Done".into(),
                terminal: true,
            },
            StatusDef {
                id: StatusId("blocked".into()),
                label: "Blocked".into(),
                terminal: false,
            },
        ],
        executor: ExecutorConfig {
            default: "claude".into(),
            mode: "accept".into(),
            sandbox: SandboxConfig::default(),
            resolver: ResolverConfig::default(),
        },
        packer: PackerConfig {
            token_budget: 80_000,
            style: "markdown".into(),
            ignore_patterns: vec![],
        },
        context_template: "# {{ id }}".into(),
        plan_template: "Plan for {{ id }}".into(),
        tester: None,
        spawn: SpawnConfig {
            max_parallel: 3,
            isolation: "worktree".into(),
            base_branch: "main".into(),
            branch_prefix: "tc/".into(),
            auto_commit: false,
            on_complete: "pr".into(),
        },
        verification: VerificationConfig::default(),
    }
}

pub fn dummy_task(id: &str, epic: &str, status: &str) -> Task {
    Task {
        id: TaskId(id.into()),
        title: format!("Title for {id}"),
        epic: epic.into(),
        status: StatusId(status.into()),
        priority: tc_core::task::Priority::default(),
        depends_on: vec![],
        files: vec![],
        pack_exclude: vec![],
        notes: String::new(),
        acceptance_criteria: vec![],
        assignee: None,
        created_at: Utc::now(),
    }
}

/// Build an App without touching the filesystem (used by snapshot tests).
pub fn app_with(tasks: Vec<Task>) -> App {
    let config = dummy_config();
    let dag = TaskDag::from_tasks(&tasks).expect("dag");
    let sm = StatusMachine::new(config.statuses.clone());
    let store_root = std::env::temp_dir().join(format!("tc-tui-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(store_root.join(".tc"));
    let store = Store::open(store_root).expect("store");
    let _ = std::fs::remove_file(store.draft_add_task_path());
    let mut app = App {
        store,
        config,
        tasks,
        dag,
        status_machine: sm,
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
        max_workers: 3,
        input_mode: InputMode::Normal,
        input: Editor::new(),
        filter: String::new(),
        status_message: "ready".into(),
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
    app
}
