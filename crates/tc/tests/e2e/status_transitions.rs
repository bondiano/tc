use crate::helpers::{setup_project, tc_run};

#[test]
fn full_lifecycle() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Lifecycle task", "--epic", "test"]);

    // todo -> in_progress
    tc_run(root, &["status", "T-001", "in_progress"]);
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("in_progress") || out.contains("In Progress"));

    // in_progress -> review
    tc_run(root, &["status", "T-001", "review"]);
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("review") || out.contains("Review"));

    // review -> done
    tc_run(root, &["done", "T-001"]);
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("done") || out.contains("Done"));
}

#[test]
fn block_and_unblock() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Blockable task", "--epic", "test"]);

    // block with reason
    tc_run(root, &["block", "T-001", "--reason", "waiting for API key"]);
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("blocked") || out.contains("Blocked"));
    assert!(out.contains("waiting for API key"));

    // unblock: status back to todo
    tc_run(root, &["status", "T-001", "todo"]);
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("todo") || out.contains("Todo"));

    // can now complete
    tc_run(root, &["done", "T-001"]);
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("done") || out.contains("Done"));
}

#[test]
fn done_unblocks_dependents() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Dependency", "--epic", "test"]);
    tc_run(
        root,
        &[
            "add",
            "Dependent task",
            "--epic",
            "test",
            "--after",
            "T-001",
        ],
    );

    // T-002 should not be ready
    let out = tc_run(root, &["list", "--ready"]);
    assert!(
        !out.contains("Dependent task"),
        "T-002 should not be ready yet"
    );

    // Complete T-001
    tc_run(root, &["done", "T-001"]);

    // T-002 should now be ready
    let out = tc_run(root, &["list", "--ready"]);
    assert!(
        out.contains("Dependent task"),
        "T-002 should be ready after T-001 done"
    );
}

#[test]
fn impl_dry_run_shows_context() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(
        root,
        &[
            "add",
            "Feature with AC",
            "--epic",
            "core",
            "--ac",
            "All tests pass",
            "--ac",
            "No regressions",
        ],
    );

    let out = tc_run(root, &["impl", "T-001", "--dry-run"]);

    // Default context template renders task title, epic, and checklist
    assert!(
        out.contains("Feature with AC"),
        "dry-run should show task title: {out}"
    );
    assert!(out.contains("core"), "dry-run should show epic: {out}");
    assert!(
        out.contains("Checklist"),
        "dry-run should show checklist section: {out}"
    );
    assert!(
        out.contains("tc done T-001"),
        "dry-run should show done command: {out}"
    );
}
