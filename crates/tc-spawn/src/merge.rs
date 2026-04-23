use std::path::Path;
use std::process::Command;

use tc_core::config::TcConfig;
use tc_core::task::TaskId;
use tc_executor::traits::Executor;

use crate::error::SpawnError;
use crate::resolver::{ResolveContext, ResolveOutcome, try_resolve_rebase_conflict};
use crate::worktree::WorktreeManager;

#[derive(Debug)]
pub enum MergeResult {
    Success,
    Conflict { details: String },
}

/// Merge a task's worktree branch back into the base branch using a
/// pre-rebase + squash-merge strategy for flat history.
///
/// Algorithm:
///   1. In the worktree: `git rebase <base>`. On conflict, `git rebase --abort`
///      (best-effort) and return `MergeResult::Conflict`. Git worktrees share
///      refs with the primary repo, so `<base>` already reflects the latest
///      primary-repo state -- no fetch needed.
///   2. In the main repo: `git merge --squash <branch>` stages the diff without
///      a commit.
///   3. In the main repo: `git commit -m "{task_title} ({task_id})"` -- skipped
///      if there were no staged changes (branch was empty of real work).
///   4. Remove the worktree and branch via `WorktreeManager::remove`.
///   5. Remove the worker state file (best-effort).
///
/// Callers must serialize calls to this function: parallel rebases against a
/// moving base branch would race.
pub fn merge_worktree(
    mgr: &WorktreeManager,
    task_id: &TaskId,
    task_title: &str,
) -> Result<MergeResult, SpawnError> {
    match preflight_sync(mgr, task_id)? {
        Preflight::Ready(ctx) => finish_merge(mgr, task_id, task_title, &ctx),
        Preflight::Abort(result) => Ok(result),
    }
}

/// Async variant of `merge_worktree` that consults
/// `config.executor.resolver`. When enabled, a rebase conflict is handed to
/// the configured agent in Yolo + sandbox mode; if the agent resolves every
/// file, the rebase continues and the squash proceeds as normal. Otherwise
/// the rebase is aborted and `MergeResult::Conflict` is returned -- identical
/// shape to the synchronous path.
pub async fn merge_worktree_resolving<E: Executor>(
    mgr: &WorktreeManager,
    task_id: &TaskId,
    task_title: &str,
    config: &TcConfig,
    executor: &E,
) -> Result<MergeResult, SpawnError> {
    match preflight_resolving(mgr, task_id, task_title, config, executor).await? {
        Preflight::Ready(ctx) => finish_merge(mgr, task_id, task_title, &ctx),
        Preflight::Abort(result) => Ok(result),
    }
}

struct MergeCtx {
    branch: String,
    wt_path: std::path::PathBuf,
    base_branch: String,
    main_root: std::path::PathBuf,
}

enum RebaseOutcome {
    Ok,
    Conflict { details: String },
}

/// Result of pre-merge checks: either the worktree is rebased cleanly and
/// ready for the squash step, or the merge is aborted with a final result.
enum Preflight {
    Ready(MergeCtx),
    Abort(MergeResult),
}

fn preflight_sync(mgr: &WorktreeManager, task_id: &TaskId) -> Result<Preflight, SpawnError> {
    let ctx = prepare_merge(mgr, task_id)?;
    match run_rebase(&ctx.wt_path, &ctx.base_branch)? {
        RebaseOutcome::Ok => Ok(Preflight::Ready(ctx)),
        RebaseOutcome::Conflict { details } => {
            abort_rebase(&ctx.wt_path);
            Ok(Preflight::Abort(MergeResult::Conflict { details }))
        }
    }
}

async fn preflight_resolving<E: Executor>(
    mgr: &WorktreeManager,
    task_id: &TaskId,
    task_title: &str,
    config: &TcConfig,
    executor: &E,
) -> Result<Preflight, SpawnError> {
    let ctx = prepare_merge(mgr, task_id)?;
    let details = match run_rebase(&ctx.wt_path, &ctx.base_branch)? {
        RebaseOutcome::Ok => return Ok(Preflight::Ready(ctx)),
        RebaseOutcome::Conflict { details } => details,
    };

    if !config.executor.resolver.enabled {
        abort_rebase(&ctx.wt_path);
        return Ok(Preflight::Abort(MergeResult::Conflict { details }));
    }

    let resolve_ctx = ResolveContext {
        task_id,
        task_title,
        worktree: &ctx.wt_path,
        base_branch: &ctx.base_branch,
        merge_details: &details,
        config,
    };

    match try_resolve_rebase_conflict(resolve_ctx, executor).await? {
        ResolveOutcome::Resolved { .. } => Ok(Preflight::Ready(ctx)),
        ResolveOutcome::GaveUp { .. } | ResolveOutcome::Disabled => {
            Ok(Preflight::Abort(MergeResult::Conflict { details }))
        }
    }
}

fn prepare_merge(mgr: &WorktreeManager, task_id: &TaskId) -> Result<MergeCtx, SpawnError> {
    let info = mgr
        .find(task_id)?
        .ok_or_else(|| SpawnError::WorktreeNotFound {
            task: task_id.0.clone(),
        })?;

    Ok(MergeCtx {
        branch: info.branch.clone(),
        wt_path: info.path.clone(),
        base_branch: mgr.config().base_branch.clone(),
        main_root: mgr.root().to_path_buf(),
    })
}

fn run_rebase(wt_path: &Path, base_branch: &str) -> Result<RebaseOutcome, SpawnError> {
    let rebase = Command::new("git")
        .args(["rebase", base_branch])
        .current_dir(wt_path)
        .output()
        .map_err(|e| SpawnError::git("git rebase", e.to_string()))?;

    if rebase.status.success() {
        return Ok(RebaseOutcome::Ok);
    }

    let stdout = String::from_utf8_lossy(&rebase.stdout);
    let stderr = String::from_utf8_lossy(&rebase.stderr);
    let details = format!("{stdout}\n{stderr}").trim().to_string();
    Ok(RebaseOutcome::Conflict { details })
}

fn abort_rebase(wt_path: &Path) {
    let _ = Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(wt_path)
        .output();
}

fn finish_merge(
    mgr: &WorktreeManager,
    task_id: &TaskId,
    task_title: &str,
    ctx: &MergeCtx,
) -> Result<MergeResult, SpawnError> {
    let squash = Command::new("git")
        .args(["merge", "--squash", &ctx.branch])
        .current_dir(&ctx.main_root)
        .output()
        .map_err(|e| SpawnError::git("git merge --squash", e.to_string()))?;

    if !squash.status.success() {
        let stdout = String::from_utf8_lossy(&squash.stdout);
        let stderr = String::from_utf8_lossy(&squash.stderr);
        let details = format!("{stdout}\n{stderr}").trim().to_string();

        let _ = Command::new("git")
            .args(["reset", "--hard", "HEAD"])
            .current_dir(&ctx.main_root)
            .output();

        return Ok(MergeResult::Conflict { details });
    }

    let has_staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(&ctx.main_root)
        .output()
        .map_err(|e| SpawnError::git("git diff --cached", e.to_string()))?;

    if !has_staged.status.success() {
        let msg = format!("{task_title} ({id})", id = task_id.0);
        let commit = Command::new("git")
            .args(["commit", "-m", &msg])
            .current_dir(&ctx.main_root)
            .output()
            .map_err(|e| SpawnError::git("git commit", e.to_string()))?;

        if !commit.status.success() {
            let stderr = String::from_utf8_lossy(&commit.stderr);
            return Err(SpawnError::git("git commit", stderr.trim()));
        }
    }

    mgr.remove(task_id)?;

    let state_file = mgr
        .root()
        .join(".tc/workers")
        .join(format!("{}.json", task_id.0));
    let _ = std::fs::remove_file(state_file);

    Ok(MergeResult::Success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree::WorktreeManager;
    use std::process::Command as StdCommand;
    use tc_core::config::SpawnConfig;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        StdCommand::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(root)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(root)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["checkout", "-b", "main"])
            .current_dir(root)
            .output()
            .unwrap();
        // Match production: .tc/ is gitignored.
        std::fs::write(root.join(".gitignore"), ".tc/\n").unwrap();
        std::fs::write(root.join("README.md"), "# test\n").unwrap();
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        // Create .tc/ directory (gitignored).
        std::fs::create_dir_all(root.join(".tc")).unwrap();
        std::fs::write(root.join(".tc/tasks.yaml"), "tasks: []\n").unwrap();

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

    fn commit_in(path: &std::path::Path, msg: &str) {
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", msg])
            .current_dir(path)
            .output()
            .unwrap();
    }

    fn last_commit_subject(root: &std::path::Path) -> String {
        let output = StdCommand::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(root)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit_count(root: &std::path::Path) -> usize {
        let output = StdCommand::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap()
    }

    fn parent_count(root: &std::path::Path) -> usize {
        let output = StdCommand::new("git")
            .args(["log", "-1", "--pretty=%P"])
            .current_dir(root)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .count()
    }

    #[test]
    fn clean_merge_removes_worktree() {
        let dir = setup_git_repo();
        let root = dir.path();
        let mgr = WorktreeManager::new(root.to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-001".into());

        let wt_path = mgr.create(&task_id).unwrap();
        std::fs::write(wt_path.join("new_file.txt"), "hello").unwrap();
        commit_in(&wt_path, "add new_file");

        let result = merge_worktree(&mgr, &task_id, "Add new file").unwrap();
        assert!(matches!(result, MergeResult::Success));

        // Worktree should be removed.
        let list = mgr.list().unwrap();
        assert!(list.is_empty());

        // Merged file should exist on main.
        assert!(root.join("new_file.txt").exists());

        // Commit message follows "{title} ({id})" format.
        assert_eq!(last_commit_subject(root), "Add new file (T-001)");

        // Squash -> single parent (not a merge commit -> flat history).
        assert_eq!(parent_count(root), 1);
    }

    #[test]
    fn rebase_picks_up_main_advances_before_squash() {
        let dir = setup_git_repo();
        let root = dir.path();
        let mgr = WorktreeManager::new(root.to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-010".into());

        // Worktree makes a change on its own file.
        let wt_path = mgr.create(&task_id).unwrap();
        std::fs::write(wt_path.join("feature.rs"), "// feat\n").unwrap();
        commit_in(&wt_path, "add feature");

        // Main advances with an unrelated file after the worktree was created.
        std::fs::write(root.join("other.txt"), "other\n").unwrap();
        commit_in(root, "add other");

        let before = commit_count(root);

        let result = merge_worktree(&mgr, &task_id, "Add feature").unwrap();
        assert!(matches!(result, MergeResult::Success));

        // Both files exist on main.
        assert!(root.join("feature.rs").exists());
        assert!(root.join("other.txt").exists());

        // Exactly one new commit was added on top of main (the squashed one).
        assert_eq!(commit_count(root), before + 1);
        assert_eq!(last_commit_subject(root), "Add feature (T-010)");
        assert_eq!(parent_count(root), 1);
    }

    #[test]
    fn conflict_preserves_worktree() {
        let dir = setup_git_repo();
        let root = dir.path();
        let mgr = WorktreeManager::new(root.to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-002".into());

        // Worktree edits README.
        let wt_path = mgr.create(&task_id).unwrap();
        std::fs::write(wt_path.join("README.md"), "# worktree change\n").unwrap();
        commit_in(&wt_path, "modify readme in worktree");

        // Main edits README the other way (creates real conflict).
        std::fs::write(root.join("README.md"), "# main change\n").unwrap();
        commit_in(root, "modify readme on main");

        let result = merge_worktree(&mgr, &task_id, "Modify readme").unwrap();
        assert!(matches!(result, MergeResult::Conflict { .. }));

        // Worktree still exists after failed rebase.
        let list = mgr.list().unwrap();
        assert_eq!(list.len(), 1);

        // Main's README wasn't touched.
        let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
        assert_eq!(readme, "# main change\n");
    }

    #[test]
    fn empty_branch_merges_without_commit() {
        let dir = setup_git_repo();
        let root = dir.path();
        let mgr = WorktreeManager::new(root.to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-003".into());

        // Create the worktree but make no commits in it.
        mgr.create(&task_id).unwrap();

        let before = commit_count(root);
        let result = merge_worktree(&mgr, &task_id, "Empty task").unwrap();
        assert!(matches!(result, MergeResult::Success));

        // No commit was added since nothing was staged.
        assert_eq!(commit_count(root), before);

        // Worktree is still cleaned up.
        assert!(mgr.list().unwrap().is_empty());
    }

    #[test]
    fn merge_nonexistent_worktree_fails() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-999".into());

        let err = merge_worktree(&mgr, &task_id, "Missing").unwrap_err();
        assert!(matches!(err, SpawnError::WorktreeNotFound { .. }));
    }
}
