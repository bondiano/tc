use crate::helpers::{setup_project, tc_run};

#[test]
fn full_cli_workflow() {
    let dir = setup_project();
    let root = dir.path();

    // Add 3 tasks: T-002 depends on T-001, T-003 depends on T-002
    tc_run(root, &["add", "Setup database", "--epic", "backend"]);
    tc_run(
        root,
        &[
            "add",
            "Create API endpoints",
            "--epic",
            "backend",
            "--after",
            "T-001",
        ],
    );
    tc_run(
        root,
        &[
            "add",
            "Add authentication",
            "--epic",
            "backend",
            "--after",
            "T-002",
        ],
    );

    // list shows all 3
    let out = tc_run(root, &["list"]);
    assert!(out.contains("Setup database"), "list should show T-001");
    assert!(
        out.contains("Create API endpoints"),
        "list should show T-002"
    );
    assert!(out.contains("Add authentication"), "list should show T-003");

    // list --ready shows only T-001 (others blocked by deps)
    let out = tc_run(root, &["list", "--ready"]);
    assert!(out.contains("Setup database"), "T-001 should be ready");
    assert!(
        !out.contains("Create API endpoints"),
        "T-002 should not be ready"
    );

    // show T-001
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("Setup database"));

    // done T-001
    tc_run(root, &["done", "T-001"]);

    // next should suggest T-002 (now unblocked)
    let out = tc_run(root, &["next"]);
    assert!(
        out.contains("T-002") || out.contains("Create API endpoints"),
        "next should suggest T-002"
    );

    // validate (success message goes to stderr)
    let output = crate::helpers::tc_in(root)
        .arg("validate")
        .output()
        .expect("validate");
    assert!(output.status.success(), "validate should pass");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DAG valid"),
        "validate should report DAG valid: {stderr}"
    );

    // stats
    let out = tc_run(root, &["stats"]);
    assert!(out.contains("backend"), "stats should show epic");

    // graph --dot
    let out = tc_run(root, &["graph", "--dot"]);
    assert!(out.contains("digraph"), "dot output should contain digraph");
}

#[test]
fn add_with_all_fields() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "First task", "--epic", "setup"]);
    tc_run(
        root,
        &[
            "add",
            "Implement feature",
            "--epic",
            "core",
            "--after",
            "T-001",
            "--files",
            "src/main.rs,src/lib.rs",
            "--ac",
            "Tests pass",
            "--ac",
            "No warnings",
        ],
    );

    let out = tc_run(root, &["show", "T-002"]);
    assert!(out.contains("Implement feature"));
    assert!(out.contains("core"));
    assert!(out.contains("T-001"));
    assert!(out.contains("src/main.rs"));
    assert!(out.contains("Tests pass"));
    assert!(out.contains("No warnings"));
}

#[test]
fn list_filters_work() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Backend task", "--epic", "backend"]);
    tc_run(root, &["add", "Frontend task", "--epic", "frontend"]);
    tc_run(
        root,
        &[
            "add",
            "Blocked task",
            "--epic",
            "backend",
            "--after",
            "T-001",
        ],
    );

    // --epic filter
    let out = tc_run(root, &["list", "--epic", "frontend"]);
    assert!(out.contains("Frontend task"));
    assert!(!out.contains("Backend task"));

    // --ready: T-001 and T-002 ready, T-003 blocked
    let out = tc_run(root, &["list", "--ready"]);
    assert!(out.contains("Backend task"));
    assert!(out.contains("Frontend task"));
    assert!(!out.contains("Blocked task"));

    // block T-001, then --blocked filter
    tc_run(root, &["block", "T-001", "--reason", "waiting for review"]);
    let out = tc_run(root, &["list", "--blocked"]);
    assert!(out.contains("Backend task") || out.contains("T-001"));
}

#[test]
fn epic_commands() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Task A", "--epic", "auth"]);
    tc_run(root, &["add", "Task B", "--epic", "auth"]);
    tc_run(root, &["add", "Task C", "--epic", "api"]);

    // epic list
    let out = tc_run(root, &["epic", "list"]);
    assert!(out.contains("auth"));
    assert!(out.contains("api"));

    // epic show
    let out = tc_run(root, &["epic", "show", "auth"]);
    assert!(out.contains("Task A"));
    assert!(out.contains("Task B"));
    assert!(!out.contains("Task C"));

    // epic rename
    tc_run(root, &["epic", "rename", "auth", "authentication"]);
    let out = tc_run(root, &["epic", "list"]);
    assert!(out.contains("authentication"));
    assert!(!out.contains("auth\n")); // old name gone (careful with substring)

    // renamed epic's tasks still visible
    let out = tc_run(root, &["epic", "show", "authentication"]);
    assert!(out.contains("Task A"));
}

#[test]
fn config_roundtrip() {
    let dir = setup_project();
    let root = dir.path();

    // config list works
    let out = tc_run(root, &["config", "list"]);
    assert!(!out.is_empty());

    // config get
    let out = tc_run(root, &["config", "get", "executor.default"]);
    assert!(out.contains("claude"));

    // config set + verify
    tc_run(root, &["config", "set", "spawn.max_parallel", "5"]);
    let out = tc_run(root, &["config", "get", "spawn.max_parallel"]);
    assert!(out.contains('5'));

    // config reset
    tc_run(root, &["config", "reset"]);
    let out = tc_run(root, &["config", "get", "spawn.max_parallel"]);
    assert!(out.contains('3'), "should reset to default 3: {out}");
}

#[test]
fn completion_outputs() {
    let dir = setup_project();
    let root = dir.path();

    for shell in &["bash", "zsh", "fish"] {
        let out = tc_run(root, &["completion", shell]);
        assert!(!out.is_empty(), "{shell} completion should not be empty");
        assert!(
            out.contains("tc"),
            "{shell} completion should reference tc: {out}"
        );
    }
}

#[test]
fn list_ids_only_for_completion() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "First task", "--epic", "a"]);
    tc_run(root, &["add", "Second task", "--epic", "b"]);
    tc_run(root, &["add", "Third task", "--epic", "a"]);

    let out = tc_run(root, &["list", "--ids-only"]);
    assert_eq!(out.trim(), "T-001\nT-002\nT-003");
    assert!(!out.contains("First task"));

    // Combined with filter: --ids-only should honor --epic
    let out = tc_run(root, &["list", "--ids-only", "--epic", "a"]);
    assert_eq!(out.trim(), "T-001\nT-003");
}
