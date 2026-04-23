use std::fs;

use crate::helpers::{setup_git_project, tc_run};

#[test]
fn pack_whole_project() {
    let dir = setup_git_project();
    let root = dir.path();

    // Create source files
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main.rs");
    fs::write(root.join("src/lib.rs"), "pub fn hello() {}\n").expect("write lib.rs");
    fs::write(root.join("secret.env"), "API_KEY=sk-secret\n").expect("write secret.env");

    // Add .gitignore excluding secret.env
    fs::write(root.join(".gitignore"), "secret.env\n.tc/\n").expect("write .gitignore");

    let out = tc_run(root, &["pack"]);

    // Should include source files
    assert!(out.contains("src/main.rs"), "pack should include main.rs");
    assert!(out.contains("src/lib.rs"), "pack should include lib.rs");

    // Should exclude gitignored file
    assert!(
        !out.contains("secret.env"),
        "pack should exclude gitignored files"
    );
}

#[test]
fn pack_scoped_to_task() {
    let dir = setup_git_project();
    let root = dir.path();

    // Create source files
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main.rs");
    fs::write(root.join("src/lib.rs"), "pub fn hello() {}\n").expect("write lib.rs");
    fs::write(root.join("src/utils.rs"), "pub fn util() {}\n").expect("write utils.rs");

    // Add task scoped to specific file
    tc_run(
        root,
        &["add", "Fix main", "--files", "src/main.rs", "--epic", "fix"],
    );

    let out = tc_run(root, &["pack", "T-001"]);

    // Should include scoped file
    assert!(
        out.contains("src/main.rs"),
        "pack T-001 should include src/main.rs"
    );

    // Should not include unrelated files
    assert!(
        !out.contains("src/utils.rs"),
        "pack T-001 should not include unrelated files"
    );
}

#[test]
fn pack_estimate() {
    let dir = setup_git_project();
    let root = dir.path();

    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main.rs");

    let out = tc_run(root, &["pack", "--estimate"]);

    // Estimate output should contain token count and file count
    assert!(
        out.contains("token") || out.contains("Token"),
        "estimate should mention tokens: {out}"
    );
    assert!(
        out.contains("file") || out.contains("File"),
        "estimate should mention files: {out}"
    );
}
