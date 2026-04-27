use std::process::{Command, Stdio};

fn tc_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tc"))
}

fn tc_in(dir: &std::path::Path) -> Command {
    let mut cmd = tc_cmd();
    cmd.current_dir(dir);
    cmd.env("NO_COLOR", "1");
    cmd.env("TC_NO_TMUX", "1");
    cmd
}

fn setup_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let output = tc_in(dir.path()).arg("init").output().unwrap();
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

#[test]
fn init_creates_tc_dir() {
    let dir = tempfile::tempdir().unwrap();
    let output = tc_in(dir.path()).arg("init").output().unwrap();
    assert!(output.status.success());
    assert!(dir.path().join(".tc").exists());
    assert!(dir.path().join(".tc/tasks.yaml").exists());
    assert!(dir.path().join(".tc/config.yaml").exists());
}

#[test]
fn init_already_initialized() {
    let dir = setup_project();
    let output = tc_in(dir.path()).arg("init").output().unwrap();
    // Should succeed gracefully (warning, not error)
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already initialized"));
}

#[test]
fn add_task() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["add", "My Task", "--epic", "backend"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-001"));
    assert!(stderr.contains("My Task"));
}

#[test]
fn add_task_with_deps() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    let output = tc_in(dir.path())
        .args(["add", "Second", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-002"));
}

#[test]
fn add_task_with_files() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args([
            "add",
            "With files",
            "--epic",
            "fe",
            "--files",
            "src/ui/,src/api/",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn add_task_with_priority() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["add", "Critical Task", "--epic", "be", "--priority", "p1"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("p1"));
}

#[test]
fn add_task_default_priority() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Normal Task", "--epic", "be"])
        .output()
        .unwrap();

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("p3"));
}

#[test]
fn list_empty() {
    let dir = setup_project();
    let output = tc_in(dir.path()).arg("list").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No tasks"));
}

#[test]
fn list_with_tasks() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "fe"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).arg("list").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Task A"));
    assert!(stdout.contains("Task B"));
    assert!(stdout.contains("[be]"));
    assert!(stdout.contains("[fe]"));
}

#[test]
fn list_filter_by_epic() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "fe"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["list", "--epic", "be"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Task A"));
    assert!(!stdout.contains("Task B"));
}

#[test]
fn list_ready_filter() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Task A"));
    assert!(!stdout.contains("Task B"));
}

#[test]
fn list_ids_only_prints_ids() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "fe"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["list", "--ids-only"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "T-001\nT-002");
    assert!(!stdout.contains("Task A"));
    assert!(!stdout.contains("[be]"));
}

#[test]
fn show_task() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "My Task", "--epic", "backend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("My Task"));
    assert!(stdout.contains("backend"));
}

#[test]
fn show_not_found() {
    let dir = setup_project();
    let output = tc_in(dir.path()).args(["show", "T-999"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn next_task() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Second", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).arg("next").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("First"));
}

#[test]
fn done_task() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("done"));
}

#[test]
fn done_already_done() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    let output = tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn done_shows_unblocked() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Second", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unblocked"));
    assert!(stderr.contains("T-002"));
}

#[test]
fn block_task() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["block", "T-001", "--reason", "API not ready"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("blocked"));
}

#[test]
fn status_change() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["status", "T-001", "review"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("review"));
}

#[test]
fn status_invalid() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["status", "T-001", "invalid_status"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn validate_valid_dag() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).arg("validate").output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("DAG valid"));
    assert!(stderr.contains("2 tasks"));
}

#[test]
fn stats_with_tasks() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "B", "--epic", "fe"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    let output = tc_in(dir.path()).arg("stats").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1/2 done"));
    assert!(stdout.contains("50%"));
}

#[test]
fn graph_ascii() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).arg("graph").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("T-002"));
}

#[test]
fn graph_dot() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["graph", "--dot"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("digraph tc {"));
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("->"));
}

#[test]
fn full_workflow() {
    let dir = setup_project();

    // Add tasks
    tc_in(dir.path())
        .args(["add", "Setup DB", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Build API", "--epic", "backend", "--after", "T-001"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Build UI", "--epic", "frontend"])
        .output()
        .unwrap();

    // List
    let output = tc_in(dir.path()).arg("list").output().unwrap();
    assert!(output.status.success());

    // Validate
    let output = tc_in(dir.path()).arg("validate").output().unwrap();
    assert!(output.status.success());

    // Next (should be T-001 or T-003)
    let output = tc_in(dir.path()).arg("next").output().unwrap();
    assert!(output.status.success());

    // Done T-001
    let output = tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    assert!(output.status.success());

    // Now T-002 should be ready
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-002"));
    assert!(stdout.contains("T-003"));

    // Stats
    let output = tc_in(dir.path()).arg("stats").output().unwrap();
    assert!(output.status.success());

    // Graph DOT
    let output = tc_in(dir.path()).args(["graph", "--dot"]).output().unwrap();
    assert!(output.status.success());
}

#[test]
fn add_task_with_acceptance_criteria() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args([
            "add",
            "With AC",
            "--epic",
            "be",
            "--ac",
            "API returns 200",
            "--ac",
            "Tests pass",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-001"));
    assert!(stderr.contains("With AC"));
}

#[test]
fn show_displays_acceptance_criteria() {
    let dir = setup_project();
    tc_in(dir.path())
        .args([
            "add",
            "AC Task",
            "--epic",
            "be",
            "--ac",
            "API returns 200",
            "--ac",
            "Tests pass",
        ])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Acceptance Criteria"));
    assert!(stdout.contains("API returns 200"));
    assert!(stdout.contains("Tests pass"));
}

#[test]
fn next_all_done() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    let output = tc_in(dir.path()).arg("next").output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("All done"));
}

#[test]
fn list_blocked_filter() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["block", "T-001", "--reason", "waiting"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["list", "--blocked"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Task A"));
    assert!(!stdout.contains("Task B"));
}

#[test]
fn show_with_deps_graph() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Second", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["show", "T-002"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("Deps"));
}

#[test]
fn validate_empty_dag() {
    let dir = setup_project();
    let output = tc_in(dir.path()).arg("validate").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No tasks to validate"));
}

#[test]
fn stats_empty() {
    let dir = setup_project();
    let output = tc_in(dir.path()).arg("stats").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No tasks"));
}

#[test]
fn graph_empty() {
    let dir = setup_project();
    let output = tc_in(dir.path()).arg("graph").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No tasks"));
}

#[test]
fn status_to_in_progress_and_back() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();

    // Set to in_progress
    let output = tc_in(dir.path())
        .args(["status", "T-001", "in_progress"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Set back to todo
    let output = tc_in(dir.path())
        .args(["status", "T-001", "todo"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify via show
    let output = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("todo"));
}

#[test]
fn block_adds_reason_to_notes() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["block", "T-001", "--reason", "API not ready"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("API not ready"));
}

// ── Phase 3: Spawn CLI integration tests ────────────────────────────

fn setup_git_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Init git repo
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
    std::fs::write(root.join("README.md"), "# test\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    // Init tc project
    let output = tc_in(root).arg("init").output().unwrap();
    assert!(output.status.success(), "tc init failed");

    dir
}

#[test]
fn workers_empty() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).arg("workers").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No active workers"));
}

#[test]
fn workers_cleanup_empty() {
    let dir = setup_git_project();
    let output = tc_in(dir.path())
        .args(["workers", "--cleanup"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no orphaned workers"));
}

#[test]
fn kill_no_worker() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).args(["kill", "T-001"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no worker found"));
}

#[test]
fn kill_all_empty() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).args(["kill", "--all"]).output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("killed 0 worker"));
}

#[test]
fn logs_no_log_file() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).args(["logs", "T-001"]).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no log file"));
}

#[test]
fn spawn_no_ready_tasks() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).arg("spawn").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no ready tasks"));
}

#[test]
fn review_no_worktree() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["review", "T-001"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no worktree found"));
}

#[test]
fn merge_no_id_or_all() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).arg("merge").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("task ID required"));
}

#[test]
fn merge_all_empty() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).args(["merge", "--all"]).output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no tasks ready to merge"));
}

// ── Delete ──────────────────────────────────────────────────────────

#[test]
fn delete_task_not_found() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["delete", "T-999"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-999"));
}

#[test]
fn delete_task_confirm_yes() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Doomed Task", "--epic", "be"])
        .output()
        .unwrap();

    let mut child = tc_in(dir.path())
        .args(["delete", "T-001"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Deleted T-001"));

    // Verify task is gone
    let list_output = tc_in(dir.path()).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(!stdout.contains("Doomed Task"));
}

#[test]
fn delete_task_confirm_no() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Keep Me", "--epic", "be"])
        .output()
        .unwrap();

    let mut child = tc_in(dir.path())
        .args(["delete", "T-001"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Cancelled"));

    // Verify task still exists
    let list_output = tc_in(dir.path()).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(stdout.contains("Keep Me"));
}

#[test]
fn delete_task_with_dependents_blocked() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let mut child = tc_in(dir.path())
        .args(["delete", "T-001"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-002"));
    assert!(stderr.contains("--force"));
}

#[test]
fn delete_task_force_with_dependents() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let mut child = tc_in(dir.path())
        .args(["delete", "T-001", "--force"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Deleted T-001"));

    // Verify T-002 still exists but has no deps
    let show_output = tc_in(dir.path()).args(["show", "T-002"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show_output.stdout);
    assert!(stdout.contains("Task B"));
    assert!(!stdout.contains("Depends on"));
}

// ── Edit ────────────────────────────────────────────────────────────

#[test]
fn edit_task_not_found() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["edit", "T-999"])
        .env("EDITOR", "cat")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-999"));
}

#[test]
fn edit_task_no_changes() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Edit Me", "--epic", "be"])
        .output()
        .unwrap();

    // `cat` reads but doesn't modify -> no changes
    let output = tc_in(dir.path())
        .args(["edit", "T-001"])
        .env("EDITOR", "cat")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No changes"));
}

// ── E2E: Pack command ──────────────────────────────────────────────

#[test]
fn pack_whole_project() {
    let dir = setup_git_project();
    // README.md exists in the repo
    let output = tc_in(dir.path()).arg("pack").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("README.md"));
}

#[test]
fn pack_estimate() {
    let dir = setup_git_project();
    let output = tc_in(dir.path())
        .args(["pack", "--estimate"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tokens"));
    assert!(stdout.contains("files"));
}

#[test]
fn pack_for_task_with_files() {
    let dir = setup_git_project();
    let root = dir.path();
    // Create a source file and git-add it (packer is gitignore-aware)
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    Command::new("git")
        .args(["add", "src/main.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add src"])
        .current_dir(root)
        .output()
        .unwrap();

    tc_in(root)
        .args(["add", "Build API", "--epic", "be", "--files", "src/**"])
        .output()
        .unwrap();

    let output = tc_in(root).args(["pack", "T-001"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("main.rs"));
    // README.md should NOT be in pack when scoped to task files
    assert!(!stdout.contains("README.md"));
}

#[test]
fn pack_for_epic() {
    let dir = setup_git_project();
    let root = dir.path();
    std::fs::create_dir_all(root.join("api")).unwrap();
    std::fs::write(root.join("api/handler.rs"), "// handler\n").unwrap();
    std::fs::create_dir_all(root.join("ui")).unwrap();
    std::fs::write(root.join("ui/app.rs"), "// app\n").unwrap();
    Command::new("git")
        .args(["add", "api/handler.rs", "ui/app.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add files"])
        .current_dir(root)
        .output()
        .unwrap();

    tc_in(root)
        .args(["add", "API handler", "--epic", "be", "--files", "api/**"])
        .output()
        .unwrap();
    tc_in(root)
        .args(["add", "UI app", "--epic", "fe", "--files", "ui/**"])
        .output()
        .unwrap();

    let output = tc_in(root).args(["pack", "--epic", "be"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("handler.rs"));
    assert!(!stdout.contains("app.rs"));
}

#[test]
fn pack_task_not_found() {
    let dir = setup_git_project();
    let output = tc_in(dir.path()).args(["pack", "T-999"]).output().unwrap();
    assert!(!output.status.success());
}

// ── E2E: Impl dry-run ──────────────────────────────────────────────

#[test]
fn impl_dry_run_shows_context() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args([
            "add",
            "Build the API",
            "--epic",
            "be",
            "--ac",
            "Returns 200",
        ])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["impl", "T-001", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("Build the API"));
}

#[test]
fn impl_dry_run_task_not_found() {
    let dir = setup_git_project();
    let output = tc_in(dir.path())
        .args(["impl", "T-999", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn impl_dry_run_blocked_by_deps() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Second", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["impl", "T-002", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-001"));
}

#[test]
fn impl_dry_run_already_done() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    let output = tc_in(dir.path())
        .args(["impl", "T-001", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn impl_dry_run_with_resolved_deps() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Setup DB", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Build API", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    let output = tc_in(dir.path())
        .args(["impl", "T-002", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Build API"));
}

// ── E2E: Complex dependency workflows ──────────────────────────────

#[test]
fn multi_level_dependency_chain() {
    let dir = setup_project();
    // Create a 4-level chain: T-001 -> T-002 -> T-003 -> T-004
    tc_in(dir.path())
        .args(["add", "Level 1", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Level 2", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Level 3", "--epic", "be", "--after", "T-002"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Level 4", "--epic", "be", "--after", "T-003"])
        .output()
        .unwrap();

    // Only T-001 should be ready
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Level 1"));
    assert!(!stdout.contains("Level 2"));
    assert!(!stdout.contains("Level 3"));
    assert!(!stdout.contains("Level 4"));

    // Complete T-001 -> T-002 becomes ready
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Level 2"));
    assert!(!stdout.contains("Level 3"));

    // Complete T-002 -> T-003 becomes ready
    tc_in(dir.path()).args(["done", "T-002"]).output().unwrap();
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Level 3"));
    assert!(!stdout.contains("Level 4"));

    // Complete T-003 -> T-004 becomes ready
    tc_in(dir.path()).args(["done", "T-003"]).output().unwrap();
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Level 4"));
}

#[test]
fn diamond_dependency_pattern() {
    let dir = setup_project();
    // T-001 -> T-002, T-001 -> T-003, T-002 + T-003 -> T-004
    tc_in(dir.path())
        .args(["add", "Base", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Left", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Right", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Top", "--epic", "be", "--after", "T-002,T-003"])
        .output()
        .unwrap();

    // Validate DAG
    let output = tc_in(dir.path()).arg("validate").output().unwrap();
    assert!(output.status.success());

    // Complete base -> both left and right become ready
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Left"));
    assert!(stdout.contains("Right"));
    assert!(!stdout.contains("Top"));

    // Complete only left -> top still blocked (needs right)
    tc_in(dir.path()).args(["done", "T-002"]).output().unwrap();
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Right"));
    assert!(!stdout.contains("Top"));

    // Complete right -> top becomes ready
    tc_in(dir.path()).args(["done", "T-003"]).output().unwrap();
    let output = tc_in(dir.path())
        .args(["list", "--ready"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Top"));
}

#[test]
fn cross_epic_stats() {
    let dir = setup_project();
    // Multiple epics with various statuses
    tc_in(dir.path())
        .args(["add", "BE task 1", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "BE task 2", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "FE task 1", "--epic", "frontend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "FE task 2", "--epic", "frontend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "FE task 3", "--epic", "frontend"])
        .output()
        .unwrap();

    // Complete some tasks
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    tc_in(dir.path()).args(["done", "T-003"]).output().unwrap();
    tc_in(dir.path()).args(["done", "T-004"]).output().unwrap();

    let output = tc_in(dir.path()).arg("stats").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // backend: 1/2 done
    assert!(stdout.contains("backend"));
    assert!(stdout.contains("1/2"));
    // frontend: 2/3 done
    assert!(stdout.contains("frontend"));
    assert!(stdout.contains("2/3"));
    // Overall: 3/5 done = 60%
    assert!(stdout.contains("3/5"));
    assert!(stdout.contains("60%"));
}

#[test]
fn list_groups_by_epic() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "BE work", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "FE work", "--epic", "frontend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Infra work", "--epic", "infra"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).arg("list").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[backend]"));
    assert!(stdout.contains("[frontend]"));
    assert!(stdout.contains("[infra]"));
}

// ── E2E: Spawn CLI flow (worktree-based) ───────────────────────────

#[test]
fn spawn_specific_task_ids() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "be"])
        .output()
        .unwrap();

    // Spawn requires an agent (claude/opencode), so it will fail
    // but it should get past the "no ready tasks" check
    let output = tc_in(dir.path()).args(["spawn", "T-001"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not say "no ready tasks" since we gave specific IDs
    assert!(!stderr.contains("no ready tasks"));
}

#[test]
fn review_reject_saves_feedback() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Review me", "--epic", "be"])
        .output()
        .unwrap();

    // Create worktree manually to simulate spawn
    let root = dir.path();
    let wt_dir = root.join(".tc-worktrees").join("T-001");
    std::process::Command::new("git")
        .args(["worktree", "add", "-b", "tc/T-001"])
        .arg(&wt_dir)
        .current_dir(root)
        .output()
        .unwrap();

    // Set task to review status
    tc_in(root)
        .args(["status", "T-001", "review"])
        .output()
        .unwrap();

    // Reject with feedback
    let output = tc_in(root)
        .args(["review", "T-001", "--reject", "Missing error handling"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rejected"));

    // Verify feedback saved in notes and status reset
    let output = tc_in(root).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Missing error handling"));
    assert!(stdout.contains("todo"));
}

#[test]
fn merge_worktree_via_cli() {
    let dir = setup_git_project();
    let root = dir.path();

    tc_in(root)
        .args(["add", "Feature X", "--epic", "be"])
        .output()
        .unwrap();

    // Create worktree + commit a change
    let wt_dir = root.join(".tc-worktrees").join("T-001");
    std::process::Command::new("git")
        .args(["worktree", "add", "-b", "tc/T-001"])
        .arg(&wt_dir)
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(wt_dir.join("feature_x.rs"), "// feature X\n").unwrap();
    std::process::Command::new("git")
        .args(["add", "feature_x.rs"])
        .current_dir(&wt_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "implement feature X"])
        .current_dir(&wt_dir)
        .output()
        .unwrap();

    // Set task to review for merge
    tc_in(root)
        .args(["status", "T-001", "review"])
        .output()
        .unwrap();

    // Merge via CLI
    let output = tc_in(root).args(["merge", "T-001"]).output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("merged"));

    // Feature file should be on main
    assert!(root.join("feature_x.rs").exists());

    // Task should be done
    let output = tc_in(root).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("done"));
}

#[test]
fn logs_with_content() {
    let dir = setup_git_project();
    let root = dir.path();

    // Write a log file manually
    let log_dir = root.join(".tc/logs");
    std::fs::create_dir_all(&log_dir).unwrap();
    std::fs::write(
        log_dir.join("T-001.log"),
        "Starting task T-001...\nAgent completed successfully.\n",
    )
    .unwrap();

    let output = tc_in(root).args(["logs", "T-001"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Starting task T-001"));
    assert!(stdout.contains("Agent completed successfully"));
}

// ── E2E: Full lifecycle workflow ────────────────────────────────────

#[test]
fn full_lifecycle_with_deps_and_blocking() {
    let dir = setup_project();

    // Build a project with dependencies
    tc_in(dir.path())
        .args(["add", "Design schema", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args([
            "add",
            "Implement models",
            "--epic",
            "backend",
            "--after",
            "T-001",
        ])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Build API", "--epic", "backend", "--after", "T-002"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Build UI", "--epic", "frontend"])
        .output()
        .unwrap();

    // Validate
    let output = tc_in(dir.path()).arg("validate").output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("4 tasks"));

    // Next should be T-001 or T-004 (both have no deps)
    let output = tc_in(dir.path()).arg("next").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Design schema") || stdout.contains("Build UI"));

    // Start working on T-001
    tc_in(dir.path())
        .args(["status", "T-001", "in_progress"])
        .output()
        .unwrap();

    // Block T-004 externally
    tc_in(dir.path())
        .args(["block", "T-004", "--reason", "Waiting for design mockups"])
        .output()
        .unwrap();

    // Complete T-001 -> unblocks T-002
    let output = tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unblocked"));
    assert!(stderr.contains("T-002"));

    // Check stats: 1/4 done = 25%
    let output = tc_in(dir.path()).arg("stats").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1/4"));
    assert!(stdout.contains("25%"));

    // List blocked
    let output = tc_in(dir.path())
        .args(["list", "--blocked"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Build UI"));
    assert!(!stdout.contains("Implement models"));

    // Complete T-002, T-003
    tc_in(dir.path()).args(["done", "T-002"]).output().unwrap();
    tc_in(dir.path()).args(["done", "T-003"]).output().unwrap();

    // Unblock T-004
    tc_in(dir.path())
        .args(["status", "T-004", "todo"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-004"]).output().unwrap();

    // Final stats: 4/4 done = 100%
    let output = tc_in(dir.path()).arg("stats").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("4/4"));
    assert!(stdout.contains("100%"));

    // Next should say all done
    let output = tc_in(dir.path()).arg("next").output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("All done"));
}

#[test]
fn graph_reflects_status_changes() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    // DOT graph before completing
    let output = tc_in(dir.path()).args(["graph", "--dot"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("T-002"));
    assert!(stdout.contains("->"));

    // Complete T-001
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    // DOT graph should still show the relationship
    let output = tc_in(dir.path()).args(["graph", "--dot"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("T-002"));
}

// ── E2E: Edge cases and error handling ─────────────────────────────

#[test]
fn add_task_missing_epic() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["add", "No epic task"])
        .output()
        .unwrap();
    // Clap should reject this (--epic is required)
    assert!(!output.status.success());
}

#[test]
fn add_task_with_nonexistent_dep_then_validate() {
    let dir = setup_project();
    // add doesn't validate deps, but validate catches orphan references
    let output = tc_in(dir.path())
        .args(["add", "Task", "--epic", "be", "--after", "T-999"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // validate should flag the invalid dependency
    let output = tc_in(dir.path()).arg("validate").output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn status_transition_from_blocked_to_todo() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["block", "T-001", "--reason", "external"])
        .output()
        .unwrap();

    // Should be able to unblock by setting to todo
    let output = tc_in(dir.path())
        .args(["status", "T-001", "todo"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("todo"));
}

#[test]
fn multiple_acceptance_criteria_shown() {
    let dir = setup_project();
    tc_in(dir.path())
        .args([
            "add",
            "Complex task",
            "--epic",
            "be",
            "--ac",
            "API returns 200",
            "--ac",
            "Tests pass",
            "--ac",
            "Logs are written",
        ])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("API returns 200"));
    assert!(stdout.contains("Tests pass"));
    assert!(stdout.contains("Logs are written"));
}

#[test]
fn done_unblocks_multiple_dependents() {
    let dir = setup_project();
    // T-001 blocks both T-002 and T-003
    tc_in(dir.path())
        .args(["add", "Base", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Dep A", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Dep B", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unblocked"));
    assert!(stderr.contains("T-002"));
    assert!(stderr.contains("T-003"));
}

#[test]
fn merge_no_worktree() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["merge", "T-001"]).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn spawn_with_epic_filter_no_matching() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "backend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["spawn", "--epic", "frontend"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no ready tasks"));
}

#[test]
fn delete_and_readd_preserves_id_sequence() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Second", "--epic", "be"])
        .output()
        .unwrap();

    // Delete T-001
    let mut child = tc_in(dir.path())
        .args(["delete", "T-001"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    child.wait_with_output().unwrap();

    // Add new task -> should get T-003 (not T-001)
    let output = tc_in(dir.path())
        .args(["add", "Third", "--epic", "be"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("T-003"));
}

// ── Epic subcommands ─────────────────────────────────────────────────

#[test]
fn epic_list_empty() {
    let dir = setup_project();
    let output = tc_in(dir.path()).args(["epic", "list"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No epics"));
}

#[test]
fn epic_list_shows_epics() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task C", "--epic", "frontend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["epic", "list"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend"));
    assert!(stdout.contains("frontend"));
    assert!(stdout.contains("0/2 done"));
    assert!(stdout.contains("0/1 done"));
}

#[test]
fn epic_list_tracks_progress() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    let output = tc_in(dir.path()).args(["epic", "list"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1/2 done"));
}

#[test]
fn epic_show_displays_tasks() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "backend"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task C", "--epic", "frontend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["epic", "show", "backend"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[backend]"));
    assert!(stdout.contains("Task A"));
    assert!(stdout.contains("Task B"));
    assert!(!stdout.contains("Task C"));
    assert!(stdout.contains("0/2 done"));
}

#[test]
fn epic_show_case_insensitive() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "Backend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["epic", "show", "backend"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Task A"));
}

#[test]
fn epic_show_not_found() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "backend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["epic", "show", "nonexistent"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn epic_rename() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task B", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Task C", "--epic", "fe"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["epic", "rename", "be", "backend"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Renamed"));
    assert!(stderr.contains("2 tasks"));

    // Verify rename took effect
    let output = tc_in(dir.path()).args(["epic", "list"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backend"));
    assert!(!stdout.contains(" be "));
    assert!(stdout.contains("fe"));
}

#[test]
fn epic_rename_not_found() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "backend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["epic", "rename", "nonexistent", "new-name"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn epic_rename_case_insensitive() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Task A", "--epic", "Backend"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["epic", "rename", "backend", "api"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = tc_in(dir.path()).args(["epic", "list"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("api"));
    assert!(!stdout.contains("Backend"));
}

#[test]
fn epic_show_with_ready_count() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Second", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["epic", "show", "be"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // T-001 is ready, T-002 is blocked by T-001
    assert!(stdout.contains("1 ready"));
}

#[test]
fn epic_list_shows_ready_count() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "First", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path())
        .args(["add", "Second", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path()).args(["epic", "list"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 ready"));
}
// ── Config ──────────────────────────────────────────────────────────

#[test]
fn config_list_shows_yaml() {
    let dir = setup_project();
    let output = tc_in(dir.path()).args(["config"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("executor:"));
    assert!(stdout.contains("spawn:"));
    assert!(stdout.contains("packer:"));
}

#[test]
fn config_list_subcommand() {
    let dir = setup_project();
    let output = tc_in(dir.path()).args(["config", "list"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("executor:"));
}

#[test]
fn config_get_string() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "get", "executor.default"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "claude");
}

#[test]
fn config_get_number() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "get", "spawn.max_parallel"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn config_get_bool() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "get", "spawn.auto_commit"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "true");
}

#[test]
fn config_get_nested_section() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "get", "executor"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("default:"));
    assert!(stdout.contains("claude"));
}

#[test]
fn config_get_missing_key() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "get", "nonexistent.key"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Key not found"));
}

#[test]
fn config_set_string() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "executor.default", "opencode"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify
    let output = tc_in(dir.path())
        .args(["config", "get", "executor.default"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "opencode");
}

#[test]
fn config_set_number() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "spawn.max_parallel", "5"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = tc_in(dir.path())
        .args(["config", "get", "spawn.max_parallel"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "5");
}

#[test]
fn config_set_bool() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "spawn.auto_commit", "false"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = tc_in(dir.path())
        .args(["config", "get", "spawn.auto_commit"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "false");
}

#[test]
fn config_set_missing_key() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "executor.nonexistent", "value"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Key not found"));
}

#[test]
fn config_path_shows_path() {
    let dir = setup_project();
    let output = tc_in(dir.path()).args(["config", "path"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().ends_with(".tc/config.yaml"));
}

#[test]
fn config_reset_restores_defaults() {
    let dir = setup_project();
    // Change a value
    tc_in(dir.path())
        .args(["config", "set", "spawn.max_parallel", "99"])
        .output()
        .unwrap();

    // Reset
    let output = tc_in(dir.path())
        .args(["config", "reset"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("reset"));

    // Verify default restored
    let output = tc_in(dir.path())
        .args(["config", "get", "spawn.max_parallel"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn config_without_init_fails() {
    let dir = tempfile::tempdir().unwrap();
    let output = tc_in(dir.path()).args(["config"]).output().unwrap();
    assert!(!output.status.success());
}

// ── Shell completion tests ──────────────────────────────────────────

#[test]
fn completion_bash() {
    let output = tc_cmd().args(["completion", "bash"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("tc"),
        "bash completions should reference 'tc'"
    );
}

#[test]
fn completion_zsh() {
    let output = tc_cmd().args(["completion", "zsh"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("tc"),
        "zsh completions should reference 'tc'"
    );
}

#[test]
fn completion_fish() {
    let output = tc_cmd().args(["completion", "fish"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("tc"),
        "fish completions should reference 'tc'"
    );
}

#[test]
fn completion_invalid_shell() {
    let output = tc_cmd()
        .args(["completion", "powershell-invalid"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

// ── Config validation tests ─────────────────────────────────────────

#[test]
fn config_set_invalid_executor_fails() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "executor.default", "vim"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid") || stderr.contains("Invalid"),
        "expected validation error, got: {stderr}"
    );
}

#[test]
fn config_set_invalid_mode_fails() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "executor.mode", "wat"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn config_set_invalid_pack_style_fails() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "packer.style", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn config_set_zero_max_parallel_fails() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "spawn.max_parallel", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn config_set_zero_token_budget_fails() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["config", "set", "packer.token_budget", "0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
// ── Phase 6: Plan command tests ─────────────────────────────────────

#[test]
fn plan_dry_run_shows_prompt() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args([
            "add",
            "Build the API",
            "--epic",
            "be",
            "--ac",
            "Returns 200",
        ])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["plan", "T-001", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("Build the API"));
    assert!(stdout.contains("implementation plan"));
}

#[test]
fn plan_dry_run_task_not_found() {
    let dir = setup_git_project();
    let output = tc_in(dir.path())
        .args(["plan", "T-999", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn plan_dry_run_already_done() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "Task", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();

    let output = tc_in(dir.path())
        .args(["plan", "T-001", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn plan_dry_run_includes_acceptance_criteria() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args([
            "add",
            "Build the API",
            "--epic",
            "be",
            "--ac",
            "Returns 200",
            "--ac",
            "Handles errors",
        ])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["plan", "T-001", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Returns 200"));
    assert!(stdout.contains("Handles errors"));
}

#[test]
fn plan_dry_run_with_resolved_deps() {
    let dir = setup_git_project();
    tc_in(dir.path())
        .args(["add", "First task", "--epic", "be"])
        .output()
        .unwrap();
    tc_in(dir.path()).args(["done", "T-001"]).output().unwrap();
    tc_in(dir.path())
        .args(["add", "Second task", "--epic", "be", "--after", "T-001"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["plan", "T-002", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("T-001"));
    assert!(stdout.contains("First task"));
    assert!(stdout.contains("[done]"));
}

// ── tc migrate (M-6.2) ───────────────────────────────────────────────

const LEGACY_TASKS_YAML: &str = "tasks:\n- id: T-001\n  title: Pre-6.1 task\n  epic: legacy\n  status: todo\n  assignee: null\n  created_at: 2026-01-01T00:00:00Z\n";

fn write_legacy_tasks(dir: &std::path::Path) {
    std::fs::write(dir.join(".tc/tasks.yaml"), LEGACY_TASKS_YAML).unwrap();
}

#[test]
fn migrate_rewrites_legacy_yaml() {
    let dir = setup_project();
    write_legacy_tasks(dir.path());

    let output = tc_in(dir.path()).args(["migrate"]).output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("normalized"), "stderr was: {stderr}");

    let written = std::fs::read_to_string(dir.path().join(".tc/tasks.yaml")).unwrap();
    assert!(
        written.contains("priority: p3"),
        "migrated file should populate priority: {written}"
    );
}

#[test]
fn migrate_noop_on_already_normalized_file() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Fresh task", "--epic", "be"])
        .output()
        .unwrap();
    let before = std::fs::read_to_string(dir.path().join(".tc/tasks.yaml")).unwrap();

    let output = tc_in(dir.path()).args(["migrate"]).output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("up to date"),
        "expected no-op message, got: {stderr}"
    );

    let after = std::fs::read_to_string(dir.path().join(".tc/tasks.yaml")).unwrap();
    assert_eq!(before, after, "no-op migrate must not touch the file");
}

#[test]
fn migrate_dry_run_does_not_write() {
    let dir = setup_project();
    write_legacy_tasks(dir.path());

    let output = tc_in(dir.path())
        .args(["migrate", "--dry-run"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("priority: p3"),
        "dry-run stdout should preview normalized YAML: {stdout}"
    );

    let on_disk = std::fs::read_to_string(dir.path().join(".tc/tasks.yaml")).unwrap();
    assert_eq!(
        on_disk, LEGACY_TASKS_YAML,
        "dry-run must leave file untouched"
    );
}

#[test]
fn migrate_check_exits_nonzero_when_changes_pending() {
    let dir = setup_project();
    write_legacy_tasks(dir.path());

    let output = tc_in(dir.path())
        .args(["migrate", "--check"])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "--check must fail when migration is pending"
    );

    let on_disk = std::fs::read_to_string(dir.path().join(".tc/tasks.yaml")).unwrap();
    assert_eq!(on_disk, LEGACY_TASKS_YAML, "--check must not write");
}

#[test]
fn migrate_check_succeeds_when_file_is_current() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "Fresh task", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["migrate", "--check"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "--check must succeed on a current file"
    );
}

// ── tc add / tc edit new flags (M-6.3) ───────────────────────────────

#[test]
fn add_with_all_new_flags_round_trips_via_show() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args([
            "add",
            "Triage me",
            "--epic",
            "be",
            "--priority",
            "p1",
            "--tag",
            "backend",
            "--tag",
            "perf",
            "--due",
            "2026-06-01",
            "--scheduled",
            "2026-05-25",
            "--estimate",
            "2h",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("p1"), "show should render p1: {stdout}");
    assert!(
        stdout.contains("backend"),
        "show should render tag backend: {stdout}"
    );
    assert!(
        stdout.contains("perf"),
        "show should render tag perf: {stdout}"
    );
    assert!(
        stdout.contains("2026-06-01"),
        "show should render due: {stdout}"
    );
    assert!(
        stdout.contains("2026-05-25"),
        "show should render scheduled: {stdout}"
    );
    assert!(
        stdout.contains("2h"),
        "show should render estimate: {stdout}"
    );
}

#[test]
fn add_rejects_bad_date_format() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["add", "x", "--epic", "e", "--due", "2026/06/01"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "bad date must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("YYYY-MM-DD"),
        "error should hint format: {stderr}"
    );
}

#[test]
fn add_rejects_bad_duration() {
    let dir = setup_project();
    let output = tc_in(dir.path())
        .args(["add", "x", "--epic", "e", "--estimate", "forever"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "bad duration must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("duration"),
        "error should mention duration: {stderr}"
    );
}

#[test]
fn edit_inline_flags_skip_editor_and_apply_patch() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "before", "--epic", "be"])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args([
            "edit",
            "T-001",
            "--priority",
            "p1",
            "--tag",
            "urgent",
            "--due",
            "2026-07-04",
            "--estimate",
            "30m",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("p1"));
    assert!(stdout.contains("urgent"));
    assert!(stdout.contains("2026-07-04"));
    assert!(stdout.contains("30m"));
}

#[test]
fn edit_clear_token_removes_due_and_estimate() {
    let dir = setup_project();
    tc_in(dir.path())
        .args([
            "add",
            "with deadline",
            "--epic",
            "be",
            "--due",
            "2026-06-01",
            "--estimate",
            "2h",
        ])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["edit", "T-001", "--due", "clear", "--estimate", "clear"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(!stdout.contains("Due:"), "Due should be cleared: {stdout}");
    assert!(
        !stdout.contains("Estimate:"),
        "Estimate should be cleared: {stdout}"
    );
}

#[test]
fn edit_add_tag_and_rm_tag_preserve_others() {
    let dir = setup_project();
    tc_in(dir.path())
        .args([
            "add", "x", "--epic", "be", "--tag", "a", "--tag", "b", "--tag", "c",
        ])
        .output()
        .unwrap();

    let output = tc_in(dir.path())
        .args(["edit", "T-001", "--add-tag", "d", "--rm-tag", "b"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(stdout.contains("a"));
    assert!(stdout.contains("c"));
    assert!(stdout.contains("d"));
    // The Tags line itself is `Tags:       a, c, d`. Check that 'b' is
    // absent from THAT specific row.
    let tags_line = stdout.lines().find(|l| l.starts_with("Tags:")).unwrap();
    assert!(
        !tags_line.contains('b'),
        "tag b should be removed: {tags_line}"
    );
}

#[test]
fn edit_add_tag_does_not_duplicate() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "x", "--epic", "be", "--tag", "dup"])
        .output()
        .unwrap();

    tc_in(dir.path())
        .args(["edit", "T-001", "--add-tag", "dup"])
        .output()
        .unwrap();

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&show.stdout);
    let tags_line = stdout.lines().find(|l| l.starts_with("Tags:")).unwrap();
    let count = tags_line.matches("dup").count();
    assert_eq!(count, 1, "tag must not duplicate: {tags_line}");
}

#[test]
fn edit_no_flags_falls_through_to_editor() {
    // When VISUAL/EDITOR points at /usr/bin/false the editor exits non-zero,
    // but the *attempt* proves we took the editor path. With inline flags the
    // command would succeed without ever invoking the editor.
    let dir = setup_project();
    tc_in(dir.path())
        .args(["add", "edit me", "--epic", "be"])
        .output()
        .unwrap();

    let mut cmd = tc_in(dir.path());
    cmd.env("VISUAL", "/usr/bin/false")
        .env("EDITOR", "/usr/bin/false");
    let output = cmd.args(["edit", "T-001"]).output().unwrap();
    assert!(
        !output.status.success(),
        "editor false must propagate failure"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Editor exited"),
        "should hit editor branch: {stderr}"
    );
}

#[test]
fn add_legacy_priority_alias_still_loads_via_yaml_then_show() {
    // We can't pass --priority critical (CLI rejects) but YAML files written
    // by older tc versions used `priority: critical`. Drop one in by hand to
    // confirm the alias path keeps working end-to-end.
    let dir = setup_project();
    let yaml = "tasks:\n- id: T-001\n  title: legacy\n  epic: legacy\n  status: todo\n  priority: critical\n  assignee: null\n  created_at: 2026-01-01T00:00:00Z\n";
    std::fs::write(dir.path().join(".tc/tasks.yaml"), yaml).unwrap();

    let show = tc_in(dir.path()).args(["show", "T-001"]).output().unwrap();
    assert!(show.status.success());
    let stdout = String::from_utf8_lossy(&show.stdout);
    assert!(
        stdout.contains("p1"),
        "alias 'critical' should display as canonical p1: {stdout}"
    );
}

// ── tc ui theme (M-7.5) ──────────────────────────────────────────────

#[test]
fn ui_theme_default_is_default() {
    let dir = setup_project();
    let out = tc_in(dir.path()).args(["ui", "theme"]).output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "default");
}

#[test]
fn ui_theme_set_persists_to_config() {
    let dir = setup_project();
    let out = tc_in(dir.path())
        .args(["ui", "theme", "solarized"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cfg = std::fs::read_to_string(dir.path().join(".tc/config.yaml")).unwrap();
    assert!(cfg.contains("theme: solarized"), "config: {cfg}");

    let printed = tc_in(dir.path()).args(["ui", "theme"]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&printed.stdout).trim(), "solarized");
}

#[test]
fn ui_theme_list_marks_active() {
    let dir = setup_project();
    tc_in(dir.path())
        .args(["ui", "theme", "dim"])
        .output()
        .unwrap();
    let out = tc_in(dir.path())
        .args(["ui", "theme", "list"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("* dim"), "active marker missing: {stdout}");
    // Other presets present without marker.
    assert!(stdout.contains("  default"), "{stdout}");
    assert!(stdout.contains("  solarized"), "{stdout}");
}

#[test]
fn ui_theme_unknown_rejected() {
    let dir = setup_project();
    let out = tc_in(dir.path())
        .args(["ui", "theme", "neon"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "should reject unknown theme");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown theme"), "{stderr}");
}
