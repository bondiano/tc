use std::fs;

use crate::helpers::{setup_project, tc_fail, tc_run};

#[test]
fn done_task_not_found() {
    let dir = setup_project();
    let out = tc_fail(dir.path(), &["done", "T-999"]);
    assert!(
        out.contains("not found") || out.contains("Not found") || out.contains("T-999"),
        "should report task not found: {out}"
    );
}

#[test]
fn done_blocked_by_deps() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "First task", "--epic", "test"]);
    tc_run(
        root,
        &["add", "Second task", "--epic", "test", "--after", "T-001"],
    );

    // T-002 depends on T-001 which is not done
    let out = tc_fail(root, &["done", "T-002"]);
    assert!(
        out.contains("depend") || out.contains("blocked") || out.contains("Depend"),
        "should report dependency issue: {out}"
    );
}

#[test]
fn validate_catches_bad_yaml() {
    let dir = setup_project();
    let root = dir.path();

    // Manually write tasks with circular deps
    let tasks_yaml = r#"tasks:
  - id: "T-001"
    title: "Task A"
    epic: "test"
    status: "todo"
    depends_on:
      - "T-002"
    files: []
    pack_exclude: []
    notes: ""
    acceptance_criteria: []
    created_at: "2026-01-01T00:00:00Z"
  - id: "T-002"
    title: "Task B"
    epic: "test"
    status: "todo"
    depends_on:
      - "T-001"
    files: []
    pack_exclude: []
    notes: ""
    acceptance_criteria: []
    created_at: "2026-01-01T00:00:00Z"
"#;

    let tasks_path = root.join(".tc/tasks.yaml");
    fs::write(&tasks_path, tasks_yaml).expect("write tasks.yaml");

    let out = tc_fail(root, &["validate"]);
    assert!(
        out.contains("cycle") || out.contains("Cycle") || out.contains("circular"),
        "validate should detect cycle: {out}"
    );
}

#[test]
fn impl_dry_run_on_done_task() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Quick task", "--epic", "test"]);
    tc_run(root, &["done", "T-001"]);

    let out = tc_fail(root, &["impl", "T-001", "--dry-run"]);
    assert!(
        out.contains("done") || out.contains("terminal") || out.contains("already"),
        "should reject impl on done task: {out}"
    );
}

#[test]
fn test_task_not_found() {
    let dir = setup_project();
    let out = tc_fail(dir.path(), &["test", "T-999"]);
    assert!(
        out.contains("not found") || out.contains("Not found") || out.contains("T-999"),
        "tc test should report task not found: {out}"
    );
}

#[test]
fn invalid_plan_template_rejected() {
    let dir = setup_project();
    let root = dir.path();

    // Corrupt plan_template to an unclosed minijinja expression
    let cfg_path = root.join(".tc/config.yaml");
    let content = fs::read_to_string(&cfg_path).expect("read config");
    let corrupted = format!("{content}\nplan_template: \"{{{{ unclosed\"\n");
    fs::write(&cfg_path, corrupted).expect("write config");

    // Any command that loads config should fail with a plan_template error
    let out = tc_fail(root, &["list"]);
    assert!(
        out.contains("plan_template") || out.contains("template"),
        "should report plan_template validation error: {out}"
    );
}
