use std::path::PathBuf;
use std::process::Command;

use tc_core::config::SpawnConfig;
use tc_core::task::TaskId;
use tc_spawn::merge::{MergeResult, merge_worktree};
use tc_spawn::process::{WorkerState, WorkerStatus};
use tc_spawn::recovery;
use tc_spawn::scheduler::list_worker_states;
use tc_spawn::worktree::WorktreeManager;
use tempfile::TempDir;

// ── Test helpers ─────────────────────────────────────────────────────

fn setup_git_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["checkout", "-b", "main"])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(root.join("README.md"), "# test project\n").unwrap();
    std::fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(root)
        .output()
        .unwrap();

    // Create .tc/ directory with minimal structure
    let tc_dir = root.join(".tc");
    std::fs::create_dir_all(tc_dir.join("logs")).unwrap();
    std::fs::create_dir_all(tc_dir.join("workers")).unwrap();
    std::fs::write(tc_dir.join("tasks.yaml"), "tasks: []\n").unwrap();
    std::fs::write(tc_dir.join("config.yaml"), "version: 1\n").unwrap();

    dir
}

fn default_spawn_config() -> SpawnConfig {
    SpawnConfig {
        max_parallel: 3,
        isolation: "worktree".into(),
        base_branch: "main".into(),
        branch_prefix: "tc/".into(),
        auto_commit: false,
        on_complete: "pr".into(),
    }
}

// ── Worktree integration tests ──────────────────────────────────────

#[test]
fn worktree_full_lifecycle() {
    let dir = setup_git_repo();
    let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());

    // Create
    let id = TaskId("T-001".into());
    let wt_path = mgr.create(&id).unwrap();
    assert!(wt_path.exists());
    assert!(wt_path.join("README.md").exists());
    assert!(wt_path.join("src.rs").exists());
    assert!(wt_path.join(".tc/tasks.yaml").exists());

    // List
    let list = mgr.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].task_id, id);
    assert_eq!(list[0].branch, "tc/T-001");

    // Find
    let found = mgr.find(&id).unwrap().unwrap();
    assert_eq!(found.branch, "tc/T-001");

    // Remove
    mgr.remove(&id).unwrap();
    assert!(!wt_path.exists());
    assert!(mgr.list().unwrap().is_empty());
}

#[test]
fn worktree_parallel_create() {
    let dir = setup_git_repo();
    let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());

    let ids: Vec<TaskId> = (1..=5).map(|i| TaskId(format!("T-{i:03}"))).collect();

    for id in &ids {
        mgr.create(id).unwrap();
    }

    let list = mgr.list().unwrap();
    assert_eq!(list.len(), 5);

    // Remove all
    for id in &ids {
        mgr.remove(id).unwrap();
    }
    assert!(mgr.list().unwrap().is_empty());
}

#[test]
fn worktree_isolation_check() {
    let dir = setup_git_repo();
    let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());

    let id = TaskId("T-010".into());
    let wt_path = mgr.create(&id).unwrap();

    // Modify a file in worktree
    std::fs::write(wt_path.join("worktree_only.txt"), "hello").unwrap();
    assert!(wt_path.join("worktree_only.txt").exists());

    // File should NOT exist in main
    assert!(!dir.path().join("worktree_only.txt").exists());

    mgr.remove(&id).unwrap();
}

// ── Merge integration tests ─────────────────────────────────────────

#[test]
fn merge_clean_with_new_file() {
    let dir = setup_git_repo();
    let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());
    let id = TaskId("T-020".into());

    let wt_path = mgr.create(&id).unwrap();

    // Add new file in worktree
    std::fs::write(wt_path.join("feature.rs"), "// new feature").unwrap();
    Command::new("git")
        .args(["add", "feature.rs"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add feature"])
        .current_dir(&wt_path)
        .output()
        .unwrap();

    let result = merge_worktree(&mgr, &id, "Add feature").unwrap();
    assert!(matches!(result, MergeResult::Success));

    // Feature file should now be in main
    assert!(dir.path().join("feature.rs").exists());
    // Worktree should be gone
    assert!(mgr.list().unwrap().is_empty());
}

#[test]
fn merge_conflict_detected_and_aborted() {
    let dir = setup_git_repo();
    let root = dir.path();
    let mgr = WorktreeManager::new(root.to_path_buf(), default_spawn_config());
    let id = TaskId("T-021".into());

    let wt_path = mgr.create(&id).unwrap();

    // Modify same file in worktree
    std::fs::write(wt_path.join("README.md"), "# worktree version\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "worktree readme"])
        .current_dir(&wt_path)
        .output()
        .unwrap();

    // Modify same file on main
    std::fs::write(root.join("README.md"), "# main version\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "main readme"])
        .current_dir(root)
        .output()
        .unwrap();

    let result = merge_worktree(&mgr, &id, "Readme change").unwrap();
    assert!(matches!(result, MergeResult::Conflict { .. }));

    // Worktree should still exist after conflict
    assert_eq!(mgr.list().unwrap().len(), 1);
}

// ── Worker state tests ──────────────────────────────────────────────

#[test]
fn worker_state_persistence() {
    let dir = TempDir::new().unwrap();
    let workers_dir = dir.path().join("workers");

    let state1 = WorkerState::new(
        &TaskId("T-030".into()),
        1000,
        &PathBuf::from("/wt/T-030"),
        &PathBuf::from("/logs/T-030.log"),
    );
    let state2 = WorkerState::new(
        &TaskId("T-031".into()),
        1001,
        &PathBuf::from("/wt/T-031"),
        &PathBuf::from("/logs/T-031.log"),
    );

    state1.save(&workers_dir.join("T-030.json")).unwrap();
    state2.save(&workers_dir.join("T-031.json")).unwrap();

    let states = list_worker_states(&workers_dir).unwrap();
    assert_eq!(states.len(), 2);
}

#[test]
fn list_worker_states_empty_dir() {
    let dir = TempDir::new().unwrap();
    let states = list_worker_states(dir.path()).unwrap();
    assert!(states.is_empty());
}

#[test]
fn list_worker_states_nonexistent_dir() {
    let states = list_worker_states(&PathBuf::from("/nonexistent/workers")).unwrap();
    assert!(states.is_empty());
}

// ── Recovery tests ──────────────────────────────────────────────────

#[test]
fn recovery_scan_detects_orphaned_workers() {
    let dir = TempDir::new().unwrap();
    let workers_dir = dir.path();

    // Create a "running" worker with a PID that definitely doesn't exist
    let state = WorkerState {
        task_id: "T-040".into(),
        pid: 999_999_999,
        started_at: chrono::Utc::now(),
        worktree_path: "/wt/T-040".into(),
        status: WorkerStatus::Running,
        log_path: "/logs/T-040.log".into(),
        tmux_session: None,
    };
    state.save(&workers_dir.join("T-040.json")).unwrap();

    let orphaned = recovery::scan_orphaned_workers(workers_dir).unwrap();
    assert_eq!(orphaned.len(), 1);
    assert_eq!(orphaned[0].task_id, "T-040");
}

#[test]
fn recovery_ignores_completed_workers() {
    let dir = TempDir::new().unwrap();
    let workers_dir = dir.path();

    let state = WorkerState {
        task_id: "T-041".into(),
        pid: 999_999_999,
        started_at: chrono::Utc::now(),
        worktree_path: "/wt/T-041".into(),
        status: WorkerStatus::Completed,
        log_path: "/logs/T-041.log".into(),
        tmux_session: None,
    };
    state.save(&workers_dir.join("T-041.json")).unwrap();

    let orphaned = recovery::scan_orphaned_workers(workers_dir).unwrap();
    assert!(orphaned.is_empty());
}

#[test]
fn recovery_refresh_marks_dead_pids() {
    let dir = TempDir::new().unwrap();
    let workers_dir = dir.path();

    let state = WorkerState {
        task_id: "T-042".into(),
        pid: 999_999_999,
        started_at: chrono::Utc::now(),
        worktree_path: "/wt/T-042".into(),
        status: WorkerStatus::Running,
        log_path: "/logs/T-042.log".into(),
        tmux_session: None,
    };
    state.save(&workers_dir.join("T-042.json")).unwrap();

    let states = recovery::refresh_worker_states(workers_dir).unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].status, WorkerStatus::Failed);

    // Verify persisted
    let reloaded = WorkerState::load(&workers_dir.join("T-042.json")).unwrap();
    assert_eq!(reloaded.status, WorkerStatus::Failed);
}

#[test]
fn recovery_leaves_alive_pids() {
    let dir = TempDir::new().unwrap();
    let workers_dir = dir.path();

    // Current process PID -- definitely alive
    let state = WorkerState {
        task_id: "T-043".into(),
        pid: std::process::id(),
        started_at: chrono::Utc::now(),
        worktree_path: "/wt/T-043".into(),
        status: WorkerStatus::Running,
        log_path: "/logs/T-043.log".into(),
        tmux_session: None,
    };
    state.save(&workers_dir.join("T-043.json")).unwrap();

    let states = recovery::refresh_worker_states(workers_dir).unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].status, WorkerStatus::Running); // Still running
}

// ── File conflict detection ─────────────────────────────────────────

#[test]
fn scheduler_detects_file_conflicts() {
    use tc_core::status::StatusId;
    use tc_core::task::Task;

    let t1 = Task {
        id: TaskId("T-050".into()),
        title: "Task 1".into(),
        epic: "test".into(),
        status: StatusId("todo".into()),
        priority: tc_core::task::Priority::default(),
        depends_on: vec![],
        files: vec!["src/shared.rs".into()],
        pack_exclude: vec![],
        notes: String::new(),
        acceptance_criteria: vec![],
        assignee: None,
        created_at: chrono::Utc::now(),
    };
    let t2 = Task {
        id: TaskId("T-051".into()),
        title: "Task 2".into(),
        epic: "test".into(),
        status: StatusId("todo".into()),
        priority: tc_core::task::Priority::default(),
        depends_on: vec![],
        files: vec!["src/shared.rs".into()],
        pack_exclude: vec![],
        notes: String::new(),
        acceptance_criteria: vec![],
        assignee: None,
        created_at: chrono::Utc::now(),
    };

    use tc_executor::mock::MockExecutor;
    let tasks: Vec<&Task> = vec![&t1, &t2];
    let result = tc_spawn::scheduler::Scheduler::<MockExecutor>::detect_file_conflicts(&tasks);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("shared.rs"));
}

#[test]
fn scheduler_no_conflict_with_different_files() {
    use tc_core::status::StatusId;
    use tc_core::task::Task;

    let t1 = Task {
        id: TaskId("T-052".into()),
        title: "Task 1".into(),
        epic: "test".into(),
        status: StatusId("todo".into()),
        priority: tc_core::task::Priority::default(),
        depends_on: vec![],
        files: vec!["src/a.rs".into()],
        pack_exclude: vec![],
        notes: String::new(),
        acceptance_criteria: vec![],
        assignee: None,
        created_at: chrono::Utc::now(),
    };
    let t2 = Task {
        id: TaskId("T-053".into()),
        title: "Task 2".into(),
        epic: "test".into(),
        status: StatusId("todo".into()),
        priority: tc_core::task::Priority::default(),
        depends_on: vec![],
        files: vec!["src/b.rs".into()],
        pack_exclude: vec![],
        notes: String::new(),
        acceptance_criteria: vec![],
        assignee: None,
        created_at: chrono::Utc::now(),
    };

    use tc_executor::mock::MockExecutor;
    let tasks: Vec<&Task> = vec![&t1, &t2];
    let result = tc_spawn::scheduler::Scheduler::<MockExecutor>::detect_file_conflicts(&tasks);
    assert!(result.is_ok());
}

// ── Full spawn flow with MockExecutor ───────────────────────────────

/// End-to-end test: init project -> add tasks -> spawn with mock -> poll -> merge.
///
/// This test exercises the complete spawn pipeline without a real agent:
/// 1. Set up git repo + .tc/ with real tasks
/// 2. Spawn tasks using MockExecutor (runs `true` instead of `claude`)
/// 3. Poll workers to completion
/// 4. Verify task status transitions (todo -> in_progress -> review)
/// 5. Merge worktree back to main
/// 6. Verify cleanup (worktree removed, branch deleted)
#[tokio::test]
async fn full_spawn_poll_merge_flow() {
    use tc_core::config::TcConfig;
    use tc_core::status::{StatusDef, StatusId};
    use tc_core::task::Task;
    use tc_executor::mock::MockExecutor;
    use tc_spawn::merge::{MergeResult, merge_worktree};
    use tc_spawn::scheduler::Scheduler;

    let dir = setup_git_repo();
    let root = dir.path();
    let store = tc_storage::Store::open(root.to_path_buf()).unwrap();

    // Write tasks
    let tasks = vec![
        Task {
            id: TaskId("T-001".into()),
            title: "Add feature A".into(),
            epic: "backend".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec!["compiles".into()],
            assignee: None,
            created_at: chrono::Utc::now(),
        },
        Task {
            id: TaskId("T-002".into()),
            title: "Add feature B".into(),
            epic: "backend".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: chrono::Utc::now(),
        },
    ];
    store.save_tasks(&tasks).unwrap();

    // Build config
    let config = TcConfig {
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
                id: StatusId("review".into()),
                label: "Review".into(),
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
        executor: tc_core::config::ExecutorConfig {
            default: "claude".into(),
            mode: "accept".into(),
            sandbox: tc_core::config::SandboxConfig::default(),
            resolver: tc_core::config::ResolverConfig::default(),
        },
        packer: tc_core::config::PackerConfig {
            token_budget: 80_000,
            style: "markdown".into(),
            ignore_patterns: vec![],
        },
        context_template: "# Task {{ id }}: {{ title }}\n{{ notes }}".into(),
        plan_template: "Plan for {{ id }}: {{ title }}".into(),
        tester: None,
        spawn: default_spawn_config(),
        verification: tc_core::config::VerificationConfig::default(),
    };
    store.save_config(&config).unwrap();

    // Create scheduler with MockExecutor (exit 0 = success)
    let worktree_mgr = WorktreeManager::new(root.to_path_buf(), config.spawn.clone());
    let executor = MockExecutor::success();
    let mut scheduler = Scheduler::new(executor, worktree_mgr, 3);
    scheduler.use_tmux = false;

    // Spawn both tasks
    let spawned = scheduler
        .spawn_tasks(
            vec![TaskId("T-001".into()), TaskId("T-002".into())],
            &store,
            &config,
        )
        .await
        .unwrap();
    assert_eq!(spawned, 2);
    assert_eq!(scheduler.active_count(), 2);

    // Verify tasks moved to in_progress
    let tasks = store.load_tasks().unwrap();
    assert_eq!(tasks[0].status, StatusId("in_progress".into()));
    assert_eq!(tasks[1].status, StatusId("in_progress".into()));

    // Verify worktrees created
    let worktree_mgr = WorktreeManager::new(root.to_path_buf(), config.spawn.clone());
    let wts = worktree_mgr.list().unwrap();
    assert_eq!(wts.len(), 2);

    // Verify worker state files created
    let workers_dir = store.workers_dir();
    let states = tc_spawn::scheduler::list_worker_states(&workers_dir).unwrap();
    assert_eq!(states.len(), 2);

    // Poll workers -- MockExecutor `true` exits immediately with 0
    let completed = scheduler.poll_workers(&store, &config).await.unwrap();
    assert_eq!(completed.len(), 2);
    assert_eq!(scheduler.active_count(), 0);

    // Verify tasks moved to review (no verification commands configured)
    let tasks = store.load_tasks().unwrap();
    assert_eq!(tasks[0].status, StatusId("review".into()));
    assert_eq!(tasks[1].status, StatusId("review".into()));

    // Now merge T-001's worktree
    // First, we need to add a commit in the worktree so there's something to merge
    let wt_info = worktree_mgr.find(&TaskId("T-001".into())).unwrap().unwrap();
    std::fs::write(wt_info.path.join("feature_a.rs"), "// feature A\n").unwrap();
    Command::new("git")
        .args(["add", "feature_a.rs"])
        .current_dir(&wt_info.path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "implement feature A"])
        .current_dir(&wt_info.path)
        .output()
        .unwrap();

    let result = merge_worktree(&worktree_mgr, &TaskId("T-001".into()), "Add feature A").unwrap();
    assert!(matches!(result, MergeResult::Success));

    // Verify feature file is now on main
    assert!(root.join("feature_a.rs").exists());

    // Verify T-001 worktree removed, T-002 still present
    let wts = worktree_mgr.list().unwrap();
    assert_eq!(wts.len(), 1);
    assert_eq!(wts[0].task_id, TaskId("T-002".into()));
}

/// Test that a failing mock executor sets task to blocked.
#[tokio::test]
async fn spawn_failure_sets_blocked() {
    use tc_core::config::TcConfig;
    use tc_core::status::{StatusDef, StatusId};
    use tc_core::task::Task;
    use tc_executor::mock::MockExecutor;
    use tc_spawn::scheduler::Scheduler;

    let dir = setup_git_repo();
    let root = dir.path();
    let store = tc_storage::Store::open(root.to_path_buf()).unwrap();

    let tasks = vec![Task {
        id: TaskId("T-001".into()),
        title: "Failing task".into(),
        epic: "test".into(),
        status: StatusId("todo".into()),
        priority: tc_core::task::Priority::default(),
        depends_on: vec![],
        files: vec![],
        pack_exclude: vec![],
        notes: String::new(),
        acceptance_criteria: vec![],
        assignee: None,
        created_at: chrono::Utc::now(),
    }];
    store.save_tasks(&tasks).unwrap();

    let config = TcConfig {
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
                id: StatusId("review".into()),
                label: "Review".into(),
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
        executor: tc_core::config::ExecutorConfig {
            default: "claude".into(),
            mode: "accept".into(),
            sandbox: tc_core::config::SandboxConfig::default(),
            resolver: tc_core::config::ResolverConfig::default(),
        },
        packer: tc_core::config::PackerConfig {
            token_budget: 80_000,
            style: "markdown".into(),
            ignore_patterns: vec![],
        },
        context_template: "# Task {{ id }}: {{ title }}\n{{ notes }}".into(),
        plan_template: "Plan for {{ id }}: {{ title }}".into(),
        tester: None,
        spawn: default_spawn_config(),
        verification: tc_core::config::VerificationConfig::default(),
    };
    store.save_config(&config).unwrap();

    // MockExecutor that fails with exit code 1
    let worktree_mgr = WorktreeManager::new(root.to_path_buf(), config.spawn.clone());
    let executor = MockExecutor::failure(1);
    let mut scheduler = Scheduler::new(executor, worktree_mgr, 3);
    scheduler.use_tmux = false;

    scheduler
        .spawn_tasks(vec![TaskId("T-001".into())], &store, &config)
        .await
        .unwrap();

    // Wait for the process to actually exit
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Poll -- process exits with 1
    let completed = scheduler.poll_workers(&store, &config).await.unwrap();
    assert_eq!(completed.len(), 1);

    // Task should be blocked
    let tasks = store.load_tasks().unwrap();
    assert_eq!(tasks[0].status, StatusId("blocked".into()));
    assert!(tasks[0].notes.contains("BLOCKED"));
}

/// Test max_parallel enforcement.
#[tokio::test]
async fn spawn_respects_max_parallel() {
    use tc_core::config::TcConfig;
    use tc_core::status::{StatusDef, StatusId};
    use tc_core::task::Task;
    use tc_executor::mock::MockExecutor;
    use tc_spawn::scheduler::Scheduler;

    let dir = setup_git_repo();
    let root = dir.path();
    let store = tc_storage::Store::open(root.to_path_buf()).unwrap();

    let tasks = vec![
        Task {
            id: TaskId("T-001".into()),
            title: "Task 1".into(),
            epic: "test".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: chrono::Utc::now(),
        },
        Task {
            id: TaskId("T-002".into()),
            title: "Task 2".into(),
            epic: "test".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: chrono::Utc::now(),
        },
    ];
    store.save_tasks(&tasks).unwrap();

    let mut spawn_config = default_spawn_config();
    spawn_config.max_parallel = 1;

    let config = TcConfig {
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
                id: StatusId("review".into()),
                label: "Review".into(),
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
        executor: tc_core::config::ExecutorConfig {
            default: "claude".into(),
            mode: "accept".into(),
            sandbox: tc_core::config::SandboxConfig::default(),
            resolver: tc_core::config::ResolverConfig::default(),
        },
        packer: tc_core::config::PackerConfig {
            token_budget: 80_000,
            style: "markdown".into(),
            ignore_patterns: vec![],
        },
        context_template: "# Task {{ id }}: {{ title }}\n{{ notes }}".into(),
        plan_template: "Plan for {{ id }}: {{ title }}".into(),
        tester: None,
        spawn: spawn_config,
        verification: tc_core::config::VerificationConfig::default(),
    };
    store.save_config(&config).unwrap();

    let worktree_mgr = WorktreeManager::new(root.to_path_buf(), config.spawn.clone());
    let executor = MockExecutor::success();
    let mut scheduler = Scheduler::new(executor, worktree_mgr, 1);
    scheduler.use_tmux = false; // max_parallel = 1

    // Spawn 2 tasks but max_parallel = 1 -> only 1 spawned
    let spawned = scheduler
        .spawn_tasks(
            vec![TaskId("T-001".into()), TaskId("T-002".into())],
            &store,
            &config,
        )
        .await
        .unwrap();
    assert_eq!(spawned, 1);
    assert_eq!(scheduler.active_count(), 1);

    // Only T-001 should be in_progress
    let tasks = store.load_tasks().unwrap();
    assert_eq!(tasks[0].status, StatusId("in_progress".into()));
    assert_eq!(tasks[1].status, StatusId("todo".into()));
}

/// Test that spawn_tasks returns Ok(0) when at capacity instead of erroring.
/// Regression test for T-014: previously the caller received a MaxParallel
/// error, which caused `tc spawn` to drop queued tasks on the floor.
#[tokio::test]
async fn spawn_returns_zero_when_at_capacity() {
    use tc_core::config::TcConfig;
    use tc_core::status::{StatusDef, StatusId};
    use tc_core::task::Task;
    use tc_executor::mock::MockExecutor;
    use tc_spawn::scheduler::Scheduler;

    let dir = setup_git_repo();
    let root = dir.path();
    let store = tc_storage::Store::open(root.to_path_buf()).unwrap();

    let tasks = vec![
        Task {
            id: TaskId("T-001".into()),
            title: "Task 1".into(),
            epic: "test".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: chrono::Utc::now(),
        },
        Task {
            id: TaskId("T-002".into()),
            title: "Task 2".into(),
            epic: "test".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: chrono::Utc::now(),
        },
    ];
    store.save_tasks(&tasks).unwrap();

    let mut spawn_config = default_spawn_config();
    spawn_config.max_parallel = 1;

    let config = TcConfig {
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
                id: StatusId("review".into()),
                label: "Review".into(),
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
        executor: tc_core::config::ExecutorConfig {
            default: "claude".into(),
            mode: "accept".into(),
            sandbox: tc_core::config::SandboxConfig::default(),
            resolver: tc_core::config::ResolverConfig::default(),
        },
        packer: tc_core::config::PackerConfig {
            token_budget: 80_000,
            style: "markdown".into(),
            ignore_patterns: vec![],
        },
        context_template: "# Task {{ id }}: {{ title }}\n{{ notes }}".into(),
        plan_template: "Plan for {{ id }}: {{ title }}".into(),
        tester: None,
        spawn: spawn_config,
        verification: tc_core::config::VerificationConfig::default(),
    };
    store.save_config(&config).unwrap();

    let worktree_mgr = WorktreeManager::new(root.to_path_buf(), config.spawn.clone());
    let executor = MockExecutor::success();
    let mut scheduler = Scheduler::new(executor, worktree_mgr, 1);
    scheduler.use_tmux = false;

    // First spawn fills the single slot.
    let spawned = scheduler
        .spawn_tasks(vec![TaskId("T-001".into())], &store, &config)
        .await
        .unwrap();
    assert_eq!(spawned, 1);
    assert_eq!(scheduler.active_count(), 1);

    // Second spawn hits capacity -- should return Ok(0), NOT error. This is
    // the core behavior change: the caller re-queues instead of bailing.
    let spawned = scheduler
        .spawn_tasks(vec![TaskId("T-002".into())], &store, &config)
        .await
        .unwrap();
    assert_eq!(spawned, 0);
    assert_eq!(scheduler.active_count(), 1);

    // T-002 remains untouched in todo -- nothing silently dropped.
    let tasks = store.load_tasks().unwrap();
    let t2 = tasks
        .iter()
        .find(|t| t.id == TaskId("T-002".into()))
        .unwrap();
    assert_eq!(t2.status, StatusId("todo".into()));
}

/// Test the full queue-drain loop: spawn 3 tasks with max_parallel=1,
/// drive them to completion, verify all 3 ran (nothing dropped).
/// This is the end-to-end regression test for T-014.
#[tokio::test]
async fn spawn_drains_queue_beyond_max_parallel() {
    use tc_core::config::TcConfig;
    use tc_core::status::{StatusDef, StatusId};
    use tc_core::task::Task;
    use tc_executor::mock::MockExecutor;
    use tc_spawn::scheduler::Scheduler;

    let dir = setup_git_repo();
    let root = dir.path();
    let store = tc_storage::Store::open(root.to_path_buf()).unwrap();

    let tasks: Vec<Task> = (1..=3)
        .map(|i| Task {
            id: TaskId(format!("T-{i:03}")),
            title: format!("Task {i}"),
            epic: "test".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: chrono::Utc::now(),
        })
        .collect();
    store.save_tasks(&tasks).unwrap();

    let mut spawn_config = default_spawn_config();
    spawn_config.max_parallel = 1;

    let config = TcConfig {
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
                id: StatusId("review".into()),
                label: "Review".into(),
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
        executor: tc_core::config::ExecutorConfig {
            default: "claude".into(),
            mode: "accept".into(),
            sandbox: tc_core::config::SandboxConfig::default(),
            resolver: tc_core::config::ResolverConfig::default(),
        },
        packer: tc_core::config::PackerConfig {
            token_budget: 80_000,
            style: "markdown".into(),
            ignore_patterns: vec![],
        },
        context_template: "# Task {{ id }}: {{ title }}\n{{ notes }}".into(),
        plan_template: "Plan for {{ id }}: {{ title }}".into(),
        tester: None,
        spawn: spawn_config,
        verification: tc_core::config::VerificationConfig::default(),
    };
    store.save_config(&config).unwrap();

    let worktree_mgr = WorktreeManager::new(root.to_path_buf(), config.spawn.clone());
    let executor = MockExecutor::success();
    let mut scheduler = Scheduler::new(executor, worktree_mgr, 1);
    scheduler.use_tmux = false;

    // Drive the queue exactly the way the CLI does.
    let mut queue: std::collections::VecDeque<TaskId> =
        tasks.iter().map(|t| t.id.clone()).collect();
    let mut total_spawned = 0usize;
    let mut total_completed = 0usize;

    while !queue.is_empty() || scheduler.active_count() > 0 {
        let free = 1usize.saturating_sub(scheduler.active_count());
        let take = free.min(queue.len());
        if take > 0 {
            let batch: Vec<TaskId> = queue.drain(..take).collect();
            let n = scheduler.spawn_tasks(batch, &store, &config).await.unwrap();
            total_spawned += n;
        }

        // Give the mock process ("true") time to exit before polling.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let done = scheduler.poll_workers(&store, &config).await.unwrap();
        total_completed += done.len();

        if queue.is_empty() && scheduler.active_count() == 0 {
            break;
        }
    }

    assert_eq!(total_spawned, 3, "all 3 tasks should have been spawned");
    assert_eq!(total_completed, 3, "all 3 tasks should have completed");

    // All 3 tasks should be in review (no verification commands configured).
    let final_tasks = store.load_tasks().unwrap();
    for t in &final_tasks {
        assert_eq!(
            t.status,
            StatusId("review".into()),
            "task {} should be in review",
            t.id.0
        );
    }
}
