use std::path::{Path, PathBuf};
use std::process::Command;

use tc_core::config::SpawnConfig;
use tc_core::task::TaskId;

use crate::error::SpawnError;

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub task_id: TaskId,
    pub path: PathBuf,
    pub branch: String,
}

pub struct WorktreeManager {
    root: PathBuf,
    config: SpawnConfig,
}

impl WorktreeManager {
    pub fn new(root: PathBuf, config: SpawnConfig) -> Self {
        Self { root, config }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &SpawnConfig {
        &self.config
    }

    /// Build branch name from task id: e.g. "tc/T-001"
    fn branch_name(&self, task_id: &TaskId) -> String {
        format!("{}{}", self.config.branch_prefix, task_id.0)
    }

    /// Build worktree path: e.g. "/project/.tc-worktrees/T-001"
    fn worktree_path(&self, task_id: &TaskId) -> PathBuf {
        self.root.join(".tc-worktrees").join(&task_id.0)
    }

    /// Create a new git worktree for a task.
    ///
    /// Creates branch `{prefix}{task_id}` from `base_branch` and
    /// places the worktree at `.tc-worktrees/{task_id}`.
    /// Copies `.tc/` directory into the worktree after creation.
    pub fn create(&self, task_id: &TaskId) -> Result<PathBuf, SpawnError> {
        let wt_path = self.worktree_path(task_id);
        let branch = self.branch_name(task_id);

        if wt_path.exists() {
            return Err(SpawnError::WorktreeExists {
                task: task_id.0.clone(),
                path: wt_path,
            });
        }

        // Ensure parent directory exists
        if let Some(parent) = wt_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SpawnError::worktree_create(&wt_path, e.to_string()))?;
        }

        // git worktree add .tc-worktrees/{id} -b {prefix}{id} {base_branch}
        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                &wt_path.to_string_lossy(),
                "-b",
                &branch,
                &self.config.base_branch,
            ])
            .current_dir(&self.root)
            .output()
            .map_err(|e| SpawnError::worktree_create(&wt_path, e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SpawnError::worktree_create(&wt_path, stderr.trim()));
        }

        // Copy .tc/ directory into worktree
        copy_tc_dir(&self.root, &wt_path)?;

        Ok(wt_path)
    }

    /// Remove a worktree and its associated branch.
    pub fn remove(&self, task_id: &TaskId) -> Result<(), SpawnError> {
        let wt_path = self.worktree_path(task_id);
        let branch = self.branch_name(task_id);

        if !wt_path.exists() {
            return Err(SpawnError::WorktreeNotFound {
                task: task_id.0.clone(),
            });
        }

        // git worktree remove --force .tc-worktrees/{id}
        let output = Command::new("git")
            .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
            .current_dir(&self.root)
            .output()
            .map_err(|e| SpawnError::worktree_remove(&wt_path, e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SpawnError::worktree_remove(&wt_path, stderr.trim()));
        }

        // git branch -d {prefix}{id}  (best-effort)
        let _ = Command::new("git")
            .args(["branch", "-D", &branch])
            .current_dir(&self.root)
            .output();

        Ok(())
    }

    /// List all worktrees managed by tc (those under .tc-worktrees/).
    pub fn list(&self) -> Result<Vec<WorktreeInfo>, SpawnError> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.root)
            .output()
            .map_err(|e| SpawnError::git("git worktree list", e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SpawnError::git("git worktree list", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Canonicalize to handle symlinks (e.g. /tmp -> /private/tmp on macOS)
        let worktrees_dir = self.root.join(".tc-worktrees");
        let canonical_dir = worktrees_dir.canonicalize().unwrap_or(worktrees_dir);
        let prefix = &self.config.branch_prefix;

        Ok(parse_worktree_list(&stdout, &canonical_dir, prefix))
    }

    /// Find worktree info for a specific task.
    pub fn find(&self, task_id: &TaskId) -> Result<Option<WorktreeInfo>, SpawnError> {
        let all = self.list()?;
        Ok(all.into_iter().find(|w| w.task_id == *task_id))
    }
}

/// Parse `git worktree list --porcelain` output, filtering to tc-managed worktrees.
fn parse_worktree_list(
    output: &str,
    worktrees_dir: &Path,
    branch_prefix: &str,
) -> Vec<WorktreeInfo> {
    let mut result = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;

    'parse: for line in output.lines() {
        if let Some(wt_path) = line.strip_prefix("worktree ") {
            // Flush previous entry
            if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take())
                && let Some(info) =
                    build_worktree_info(&path, &branch, worktrees_dir, branch_prefix)
            {
                result.push(info);
            }
            current_path = Some(PathBuf::from(wt_path));
            current_branch = None;
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            // "branch refs/heads/tc/T-001" -> "tc/T-001"
            current_branch = branch_ref.strip_prefix("refs/heads/").map(String::from);
        } else if line.is_empty() {
            // Block separator -- flush
            if let (Some(path), Some(branch)) = (current_path.take(), current_branch.take())
                && let Some(info) =
                    build_worktree_info(&path, &branch, worktrees_dir, branch_prefix)
            {
                result.push(info);
            }
            continue 'parse;
        }
    }

    // Flush trailing entry
    if let (Some(path), Some(branch)) = (current_path, current_branch)
        && let Some(info) = build_worktree_info(&path, &branch, worktrees_dir, branch_prefix)
    {
        result.push(info);
    }

    result
}

/// Build WorktreeInfo if this worktree is under our managed directory.
fn build_worktree_info(
    path: &Path,
    branch: &str,
    worktrees_dir: &Path,
    branch_prefix: &str,
) -> Option<WorktreeInfo> {
    // Only include worktrees under .tc-worktrees/
    if !path.starts_with(worktrees_dir) {
        return None;
    }

    // Extract task_id from branch name (strip prefix)
    let task_id_str = branch.strip_prefix(branch_prefix)?;

    Some(WorktreeInfo {
        task_id: TaskId(task_id_str.to_string()),
        path: path.to_path_buf(),
        branch: branch.to_string(),
    })
}

/// Recursively copy .tc/ directory into the worktree.
fn copy_tc_dir(root: &Path, worktree_path: &Path) -> Result<(), SpawnError> {
    let src = root.join(".tc");
    let dst = worktree_path.join(".tc");

    if !src.exists() {
        return Ok(());
    }

    copy_dir_recursive(&src, &dst).map_err(|e| {
        SpawnError::worktree_create(worktree_path, format!("failed to copy .tc/: {e}"))
    })
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    'entries: for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
        continue 'entries;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a real git repo in a tempdir for testing.
    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

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

        // Create initial commit on main
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

        // Create .tc/ directory
        let tc_dir = root.join(".tc");
        std::fs::create_dir_all(&tc_dir).unwrap();
        std::fs::write(tc_dir.join("tasks.yaml"), "tasks: []\n").unwrap();
        std::fs::write(tc_dir.join("config.yaml"), "version: 1\n").unwrap();

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

    #[test]
    fn create_and_list_worktree() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-001".into());

        let wt_path = mgr.create(&task_id).unwrap();
        assert!(wt_path.exists());
        assert!(wt_path.join(".tc").exists());
        assert!(wt_path.join(".tc/tasks.yaml").exists());

        let list = mgr.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].task_id, task_id);
        assert_eq!(list[0].branch, "tc/T-001");
    }

    #[test]
    fn create_and_remove_worktree() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-002".into());

        let wt_path = mgr.create(&task_id).unwrap();
        assert!(wt_path.exists());

        mgr.remove(&task_id).unwrap();
        assert!(!wt_path.exists());

        let list = mgr.list().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn create_duplicate_worktree_fails() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-003".into());

        mgr.create(&task_id).unwrap();
        let err = mgr.create(&task_id).unwrap_err();
        assert!(matches!(err, SpawnError::WorktreeExists { .. }));
    }

    #[test]
    fn remove_nonexistent_worktree_fails() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-999".into());

        let err = mgr.remove(&task_id).unwrap_err();
        assert!(matches!(err, SpawnError::WorktreeNotFound { .. }));
    }

    #[test]
    fn list_empty_worktrees() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());

        let list = mgr.list().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn find_existing_worktree() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());
        let task_id = TaskId("T-004".into());

        mgr.create(&task_id).unwrap();
        let found = mgr.find(&task_id).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().task_id, task_id);
    }

    #[test]
    fn find_nonexistent_worktree() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());

        let found = mgr.find(&TaskId("T-999".into())).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn create_multiple_worktrees() {
        let dir = setup_git_repo();
        let mgr = WorktreeManager::new(dir.path().to_path_buf(), default_spawn_config());

        mgr.create(&TaskId("T-010".into())).unwrap();
        mgr.create(&TaskId("T-011".into())).unwrap();
        mgr.create(&TaskId("T-012".into())).unwrap();

        let list = mgr.list().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn parse_worktree_list_filters_non_tc() {
        let output = "\
worktree /project
HEAD abc123
branch refs/heads/main

worktree /project/.tc-worktrees/T-001
HEAD def456
branch refs/heads/tc/T-001

";
        let worktrees_dir = PathBuf::from("/project/.tc-worktrees");
        let result = parse_worktree_list(output, &worktrees_dir, "tc/");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].task_id.0, "T-001");
    }

    #[test]
    fn tc_dir_copied_into_worktree() {
        let dir = setup_git_repo();
        let root = dir.path();

        // Add nested dir to .tc/
        let logs_dir = root.join(".tc/logs");
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(logs_dir.join("test.log"), "log data").unwrap();

        let mgr = WorktreeManager::new(root.to_path_buf(), default_spawn_config());
        let wt_path = mgr.create(&TaskId("T-005".into())).unwrap();

        assert!(wt_path.join(".tc/logs/test.log").exists());
        let content = std::fs::read_to_string(wt_path.join(".tc/logs/test.log")).unwrap();
        assert_eq!(content, "log data");
    }
}
