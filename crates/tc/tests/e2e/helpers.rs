#![allow(dead_code)]

use std::path::Path;
use std::process::{Command, Output};

pub fn tc_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tc"))
}

pub fn tc_in(dir: &Path) -> Command {
    let mut cmd = tc_cmd();
    cmd.current_dir(dir);
    cmd.env("NO_COLOR", "1");
    cmd.env("TC_NO_TMUX", "1");
    cmd
}

/// Create a tempdir with `tc init`.
pub fn setup_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = tc_in(dir.path()).arg("init").output().expect("init");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

/// Create a tempdir with `git init` + initial commit + `tc init`.
pub fn setup_git_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    // git init
    let _ = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(root)
        .output()
        .expect("git init");

    let _ = Command::new("git")
        .args(["config", "user.name", "test"])
        .current_dir(root)
        .output()
        .expect("git config name");

    let _ = Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(root)
        .output()
        .expect("git config email");

    // initial commit
    std::fs::write(root.join("README.md"), "# test project\n").expect("write README");
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .expect("git add");
    let _ = Command::new("git")
        .args(["commit", "-q", "-m", "initial"])
        .current_dir(root)
        .output()
        .expect("git commit");

    // tc init
    let output = tc_in(root).arg("init").output().expect("tc init");
    assert!(
        output.status.success(),
        "tc init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    dir
}

/// Run a tc command, assert it succeeds, return stdout as String.
pub fn tc_run(dir: &Path, args: &[&str]) -> String {
    let output = tc_in(dir).args(args).output().expect("tc command");
    assert!(
        output.status.success(),
        "tc {} failed (exit {}):\nstdout: {}\nstderr: {}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Run a tc command, assert it fails, return stderr as String.
pub fn tc_fail(dir: &Path, args: &[&str]) -> String {
    let output = tc_in(dir).args(args).output().expect("tc command");
    assert!(
        !output.status.success(),
        "tc {} should have failed but succeeded:\nstdout: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
    );
    let mut combined = String::from_utf8_lossy(&output.stderr).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined
}

/// Assert command succeeded, return stdout.
pub fn assert_success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "command failed (exit {}):\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}
