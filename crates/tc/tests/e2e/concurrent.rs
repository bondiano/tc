use crate::helpers::{setup_project, tc_in, tc_run};

/// Spawn N `tc add` processes concurrently. With file locking in place, ALL N
/// adds must succeed and ALL N tasks must be persisted with unique IDs.
#[test]
fn concurrent_adds_preserve_all_tasks() {
    let dir = setup_project();
    let root = dir.path();

    let n = 6;
    let mut children = Vec::with_capacity(n);

    for i in 0..n {
        let child = tc_in(root)
            .args(["add", &format!("Concurrent task {i}"), "--epic", "race"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn tc add");
        children.push(child);
    }

    let mut successes = 0;
    for mut child in children {
        let status = child.wait().expect("wait for child");
        if status.success() {
            successes += 1;
        }
    }

    assert_eq!(
        successes, n,
        "all {n} concurrent tc add processes must succeed under locking",
    );

    // All N distinct task IDs must be present in the list.
    let out = tc_run(root, &["list"]);
    for i in 0..n {
        let needle = format!("Concurrent task {i}");
        assert!(
            out.contains(&needle),
            "task '{needle}' missing from list output:\n{out}"
        );
    }

    let validate = tc_in(root).arg("validate").output().expect("validate");
    assert!(
        validate.status.success(),
        "validate failed after concurrent adds: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
}

/// Two processes: one adds a task, the other marks a seed task as done.
/// Both touch tasks.yaml -- with locking both must succeed and neither loses work.
#[test]
fn concurrent_add_and_done_both_persist() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Seed task", "--epic", "test"]);

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

    assert!(
        add_status.status.success(),
        "add should succeed under locking: {}",
        String::from_utf8_lossy(&add_status.stderr)
    );
    assert!(
        done_status.status.success(),
        "done should succeed under locking: {}",
        String::from_utf8_lossy(&done_status.stderr)
    );

    let out = tc_run(root, &["list"]);
    assert!(
        out.contains("Seed task"),
        "seed task should still exist: {out}"
    );
    assert!(
        out.contains("New task"),
        "new task should have been persisted: {out}"
    );

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

    for i in 1..=n {
        let id = format!("T-{i:03}");
        assert!(out.contains(&id), "missing {id} in list output: {out}");
    }

    let validate = tc_in(root).arg("validate").output().expect("validate");
    assert!(validate.status.success());
}

/// Simulate a write interrupted mid-stream by verifying that atomic rename
/// leaves no partially-written tasks.yaml after repeated concurrent writers.
/// If atomicity is broken, one of the reads will fail parsing.
#[test]
fn concurrent_writers_never_leave_partial_yaml() {
    let dir = setup_project();
    let root = dir.path();

    // Seed a few so the file is non-trivial.
    for i in 0..3 {
        tc_run(root, &["add", &format!("Seed {i}"), "--epic", "atom"]);
    }

    let writers = 4;
    let mut children = Vec::with_capacity(writers);
    for i in 0..writers {
        let child = tc_in(root)
            .args(["add", &format!("Atomic {i}"), "--epic", "atom"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn writer");
        children.push(child);
    }

    // While writers race, repeatedly parse the file: every read must succeed.
    let tasks_yaml = root.join(".tc/tasks.yaml");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    let mut reads = 0;
    while std::time::Instant::now() < deadline {
        if let Ok(content) = std::fs::read_to_string(&tasks_yaml)
            && !content.is_empty()
        {
            serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&content)
                .expect("tasks.yaml must always be parseable during concurrent writes");
            reads += 1;
        }
    }

    for mut child in children {
        child.wait().expect("wait writer");
    }

    assert!(reads > 0, "the test must have exercised at least one read");

    let validate = tc_in(root).arg("validate").output().expect("validate");
    assert!(validate.status.success());
}
