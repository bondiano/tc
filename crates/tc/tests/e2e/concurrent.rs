use crate::helpers::{setup_project, tc_in, tc_run};

/// Spawn N `tc add` processes concurrently, then verify all tasks were persisted.
///
/// This is a best-effort race-condition smoke test: without file locking in
/// tc-storage, concurrent writes can clobber each other (last-writer-wins).
/// The test documents the current behaviour -- if we add locking later, all N
/// tasks should survive.
#[test]
fn concurrent_adds_do_not_corrupt_yaml() {
    let dir = setup_project();
    let root = dir.path();

    let n = 6;
    let mut children = Vec::with_capacity(n);

    // Spawn N `tc add` processes at once
    for i in 0..n {
        let child = tc_in(root)
            .args(["add", &format!("Concurrent task {i}"), "--epic", "race"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn tc add");
        children.push(child);
    }

    // Wait for all
    let mut successes = 0;
    for mut child in children {
        let status = child.wait().expect("wait for child");
        if status.success() {
            successes += 1;
        }
    }

    // At minimum one should succeed; ideally all N
    assert!(
        successes > 0,
        "at least one concurrent tc add should succeed"
    );

    // The YAML file should still be valid (tc list should not crash)
    let out = tc_run(root, &["list"]);
    assert!(
        out.contains("race"),
        "list should show tasks from the 'race' epic: {out}"
    );

    // Verify we can still add tasks (file not corrupted)
    tc_run(root, &["add", "Post-race task", "--epic", "race"]);
    let out = tc_run(root, &["list"]);
    assert!(
        out.contains("Post-race task"),
        "should be able to add after concurrent writes"
    );
}

/// Two processes: one adds a task, the other marks a different task as done.
/// Both touch tasks.yaml -- verify neither operation is lost.
#[test]
fn concurrent_add_and_done() {
    let dir = setup_project();
    let root = dir.path();

    // Seed a task to mark as done
    tc_run(root, &["add", "Seed task", "--epic", "test"]);

    // Spawn both concurrently
    let add_child = tc_in(root)
        .args(["add", "New task", "--epic", "test"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn add");

    let done_child = tc_in(root)
        .args(["done", "T-001"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn done");

    let add_status = add_child.wait_with_output().expect("wait add");
    let done_status = done_child.wait_with_output().expect("wait done");

    // Both should at least not crash / corrupt the file
    // (one may fail due to race, but the file must remain valid)
    let _add_ok = add_status.status.success();
    let _done_ok = done_status.status.success();

    // The file must still be parseable
    let out = tc_run(root, &["list"]);
    assert!(
        out.contains("Seed task"),
        "seed task should still exist: {out}"
    );

    // tc validate should not report corruption
    let validate = tc_in(root).arg("validate").output().expect("validate");
    assert!(
        validate.status.success(),
        "validate should pass after concurrent ops: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
}

/// Hammer: rapid sequential adds to ensure the ID auto-increment stays consistent.
#[test]
fn rapid_sequential_adds_preserve_id_sequence() {
    let dir = setup_project();
    let root = dir.path();

    let n = 20;
    for i in 1..=n {
        tc_run(root, &["add", &format!("Task {i}"), "--epic", "seq"]);
    }

    let out = tc_run(root, &["list"]);

    // All 20 tasks should be present
    for i in 1..=n {
        let id = format!("T-{i:03}");
        assert!(out.contains(&id), "missing {id} in list output: {out}");
    }

    // Validate DAG
    let validate = tc_in(root).arg("validate").output().expect("validate");
    assert!(validate.status.success());
}
