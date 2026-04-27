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

#[test]
fn list_with_filter_dsl() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(
        root,
        &[
            "add",
            "Urgent backend",
            "--epic",
            "be",
            "--priority",
            "p1",
            "--tag",
            "backend",
        ],
    );
    tc_run(
        root,
        &[
            "add",
            "Mid frontend",
            "--epic",
            "fe",
            "--priority",
            "p3",
            "--tag",
            "frontend",
        ],
    );
    tc_run(
        root,
        &[
            "add",
            "Low backend",
            "--epic",
            "be",
            "--priority",
            "p4",
            "--tag",
            "backend",
        ],
    );

    // priority filter
    let out = tc_run(root, &["list", "priority:p1"]);
    assert!(out.contains("Urgent backend"));
    assert!(!out.contains("Mid frontend"));
    assert!(!out.contains("Low backend"));

    // tag + priority AND
    let out = tc_run(root, &["list", "tag:backend", "priority:p4"]);
    assert!(out.contains("Low backend"));
    assert!(!out.contains("Urgent backend"));

    // negated term
    let out = tc_run(root, &["list", "!tag:frontend"]);
    assert!(out.contains("Urgent backend"));
    assert!(out.contains("Low backend"));
    assert!(!out.contains("Mid frontend"));
}

#[test]
fn smart_views_today_and_inbox() {
    let dir = setup_project();
    let root = dir.path();

    let today = chrono::Local::now().date_naive().to_string();

    tc_run(
        root,
        &[
            "add",
            "Due today task",
            "--epic",
            "today_epic",
            "--due",
            &today,
        ],
    );
    tc_run(root, &["add", "Inbox task", "--epic", "default"]);

    let out = tc_run(root, &["today"]);
    assert!(out.contains("Due today task"), "today view: {out}");
    assert!(
        !out.contains("Inbox task"),
        "today should skip undated: {out}"
    );

    let out = tc_run(root, &["inbox"]);
    assert!(out.contains("Inbox task"), "inbox view: {out}");
    assert!(!out.contains("Due today task"));
}

#[test]
fn fuzzy_find_command() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(root, &["add", "Implement OAuth login", "--epic", "auth"]);
    tc_run(root, &["add", "Refactor database layer", "--epic", "db"]);
    tc_run(root, &["add", "Fix typo in README", "--epic", "docs"]);

    let out = tc_run(root, &["find", "oauth"]);
    assert!(out.contains("Implement OAuth login"), "fuzzy find: {out}");
    assert!(!out.contains("Fix typo"));

    // ids-only mode for scripting
    let out = tc_run(root, &["find", "database", "--ids-only"]);
    assert!(out.contains("T-002"));
    assert!(!out.contains("T-001"));
}

#[test]
fn import_json_file() {
    use std::fs;
    let dir = setup_project();
    let root = dir.path();

    let json = r#"[
        {
            "title": "Imported one",
            "priority": "p1",
            "tags": ["alpha"],
            "due": "2026-12-31",
            "source_ref": "ext-1"
        },
        {
            "title": "Imported two",
            "tags": ["beta"]
        }
    ]"#;
    let path = root.join("import.json");
    fs::write(&path, json).expect("write json");

    tc_run(
        root,
        &[
            "import",
            "--format",
            "json",
            "--file",
            path.to_str().unwrap(),
            "--epic",
            "imported",
        ],
    );

    let out = tc_run(root, &["list", "--epic", "imported"]);
    assert!(out.contains("Imported one"));
    assert!(out.contains("Imported two"));

    // dedup: re-import should skip
    let stderr = String::from_utf8(
        crate::helpers::tc_in(root)
            .args([
                "import",
                "--format",
                "json",
                "--file",
                path.to_str().unwrap(),
                "--epic",
                "imported",
            ])
            .output()
            .expect("re-import")
            .stderr,
    )
    .unwrap();
    assert!(
        stderr.contains("Skipped") || stderr.contains("No new"),
        "expected dedup notice, got: {stderr}"
    );
}

#[test]
fn import_kairo_md_file() {
    use std::fs;
    let dir = setup_project();
    let root = dir.path();

    let md = "\
# My plan

- [ ] First task #backend !p1 due:2026-05-01
  - [ ] Sub item one
  - [ ] Sub item two
- [ ] Second task #frontend
- this is a noise line
";
    let path = root.join("plan.md");
    fs::write(&path, md).expect("write md");

    tc_run(
        root,
        &[
            "import",
            "--format",
            "kairo-md",
            "--file",
            path.to_str().unwrap(),
            "--epic",
            "from_md",
        ],
    );

    let out = tc_run(root, &["list", "--epic", "from_md"]);
    assert!(out.contains("First task"));
    assert!(out.contains("Second task"));

    // First task should have AC
    let out = tc_run(root, &["show", "T-001"]);
    assert!(out.contains("Sub item one"), "AC not parsed: {out}");
    assert!(out.contains("Sub item two"));
    assert!(out.contains("p1"));
}

#[test]
fn export_json_round_trips_through_import() {
    use std::fs;
    let dir = setup_project();
    let root = dir.path();

    tc_run(
        root,
        &[
            "add",
            "Alpha",
            "--epic",
            "src",
            "--priority",
            "p1",
            "--tag",
            "backend",
            "--due",
            "2026-12-31",
        ],
    );
    tc_run(
        root,
        &[
            "add",
            "Beta",
            "--epic",
            "src",
            "--priority",
            "p4",
            "--tag",
            "frontend",
            "--ac",
            "Tests pass",
        ],
    );

    let json_path = root.join("export.json");
    tc_run(
        root,
        &[
            "export",
            "--format",
            "json",
            "--output",
            json_path.to_str().unwrap(),
        ],
    );
    let exported = fs::read_to_string(&json_path).expect("read export");
    assert!(exported.contains("\"source_ref\": \"T-001\""));
    assert!(exported.contains("\"priority\": \"p1\""));
    assert!(exported.contains("\"backend\""));

    // Import into a fresh project; expect both tasks back, with their
    // priority/tag/due/AC intact.
    let dest = setup_project();
    let dest_root = dest.path();
    tc_run(
        dest_root,
        &[
            "import",
            "--format",
            "json",
            "--file",
            json_path.to_str().unwrap(),
            "--epic",
            "irrelevant_default",
        ],
    );

    let out = tc_run(dest_root, &["list"]);
    assert!(out.contains("Alpha"), "Alpha missing: {out}");
    assert!(out.contains("Beta"), "Beta missing: {out}");

    // Priority + tag + due preserved on T-001 (Alpha)
    let out = tc_run(dest_root, &["show", "T-001"]);
    assert!(out.contains("p1"));
    assert!(out.contains("backend"));
    assert!(out.contains("2026-12-31"));

    // AC preserved on T-002 (Beta)
    let out = tc_run(dest_root, &["show", "T-002"]);
    assert!(out.contains("Tests pass"));
}

#[test]
fn export_md_round_trips_through_import() {
    use std::fs;
    let dir = setup_project();
    let root = dir.path();

    tc_run(
        root,
        &[
            "add",
            "Markdown alpha",
            "--epic",
            "docs",
            "--priority",
            "p1",
            "--tag",
            "writing",
            "--ac",
            "Reviewed",
        ],
    );
    tc_run(root, &["add", "Markdown beta", "--epic", "docs"]);

    let md_path = root.join("export.md");
    tc_run(
        root,
        &[
            "export",
            "--format",
            "md",
            "--output",
            md_path.to_str().unwrap(),
        ],
    );
    let md = fs::read_to_string(&md_path).expect("read md");
    assert!(md.contains("## docs"));
    assert!(md.contains("- [ ] Markdown alpha #writing !p1"), "{md}");
    assert!(md.contains("  - [ ] Reviewed"));

    let dest = setup_project();
    let dest_root = dest.path();
    tc_run(
        dest_root,
        &[
            "import",
            "--format",
            "kairo-md",
            "--file",
            md_path.to_str().unwrap(),
            "--epic",
            "from_md",
        ],
    );

    let out = tc_run(dest_root, &["list"]);
    assert!(out.contains("Markdown alpha"));
    assert!(out.contains("Markdown beta"));

    let out = tc_run(dest_root, &["show", "T-001"]);
    assert!(out.contains("p1"));
    assert!(out.contains("writing"));
    assert!(out.contains("Reviewed"));
}

#[test]
fn export_respects_filter() {
    let dir = setup_project();
    let root = dir.path();

    tc_run(
        root,
        &["add", "Match this", "--epic", "auth", "--priority", "p1"],
    );
    tc_run(
        root,
        &["add", "Wrong epic", "--epic", "docs", "--priority", "p1"],
    );
    tc_run(
        root,
        &[
            "add",
            "Wrong priority",
            "--epic",
            "auth",
            "--priority",
            "p3",
        ],
    );

    let out = tc_run(
        root,
        &[
            "export",
            "--format",
            "json",
            "--epic",
            "auth",
            "priority:p1",
        ],
    );
    assert!(out.contains("Match this"));
    assert!(!out.contains("Wrong epic"));
    assert!(!out.contains("Wrong priority"));
}

#[test]
fn stats_shows_priority_and_today_sections() {
    let dir = setup_project();
    let root = dir.path();

    let today = chrono::Local::now().date_naive().to_string();

    tc_run(
        root,
        &["add", "Critical", "--epic", "be", "--priority", "p1"],
    );
    tc_run(root, &["add", "Normal", "--epic", "be", "--priority", "p3"]);
    tc_run(
        root,
        &["add", "On the plate", "--epic", "be", "--due", &today],
    );

    let out = tc_run(root, &["stats"]);
    assert!(
        out.contains("By priority"),
        "missing priority section: {out}"
    );
    assert!(out.contains("p1"));
    assert!(out.contains("p3"));
    assert!(out.contains("Today ("), "missing today section: {out}");
    assert!(out.contains("scheduled or due:  1"));
}
