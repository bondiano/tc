use std::path::Path;

use crate::Store;
use crate::error::{StorageError, StorageResult};

pub fn init_project(root: &Path) -> StorageResult<Store> {
    let tc_dir = root.join(".tc");

    if tc_dir.exists() {
        return Err(StorageError::AlreadyInitialized(root.to_path_buf()));
    }

    std::fs::create_dir_all(&tc_dir).map_err(|e| StorageError::dir_create(&tc_dir, e))?;
    std::fs::create_dir_all(tc_dir.join("logs"))
        .map_err(|e| StorageError::dir_create(tc_dir.join("logs"), e))?;

    let tasks_path = tc_dir.join("tasks.yaml");
    std::fs::write(&tasks_path, "tasks: []\n")
        .map_err(|e| StorageError::file_write(&tasks_path, e))?;

    let config_path = tc_dir.join("config.yaml");
    std::fs::write(&config_path, default_config())
        .map_err(|e| StorageError::file_write(&config_path, e))?;

    // Add entries to .gitignore
    let gitignore_path = root.join(".gitignore");
    let gitignore_entries = "\n# tc runtime data\n.tc/\n.tc-worktrees/\n";
    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)
            .map_err(|e| StorageError::file_read(&gitignore_path, e))?;
        if !content.contains(".tc/") {
            let mut new_content = content;
            new_content.push_str(gitignore_entries);
            std::fs::write(&gitignore_path, new_content)
                .map_err(|e| StorageError::file_write(&gitignore_path, e))?;
        }
    } else {
        std::fs::write(&gitignore_path, gitignore_entries)
            .map_err(|e| StorageError::file_write(&gitignore_path, e))?;
    }

    Store::open(root.to_path_buf())
}

pub fn default_config() -> &'static str {
    r#"statuses:
  - id: todo
    label: "Todo"
    terminal: false
  - id: in_progress
    label: "In Progress"
    terminal: false
  - id: review
    label: "Review"
    terminal: false
  - id: done
    label: "Done"
    terminal: true
  - id: blocked
    label: "Blocked"
    terminal: false

executor:
  default: claude
  mode: accept
  sandbox:
    enabled: auto
    extra_allow: []
    block_network: false

packer:
  token_budget: 80000
  style: markdown
  ignore_patterns:
    - "**/*.test.*"
    - "dist/**"
    - "node_modules/**"

spawn:
  max_parallel: 3
  isolation: worktree
  base_branch: main
  branch_prefix: "tc/"
  auto_commit: true
  on_complete: pr
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_structure() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_project(dir.path()).unwrap();

        assert!(dir.path().join(".tc").exists());
        assert!(dir.path().join(".tc/tasks.yaml").exists());
        assert!(dir.path().join(".tc/config.yaml").exists());
        assert!(dir.path().join(".tc/logs").exists());
        assert!(dir.path().join(".gitignore").exists());
        assert_eq!(store.root(), dir.path());
    }

    #[test]
    fn init_tasks_file_loadable() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_project(dir.path()).unwrap();
        let tasks = store.load_tasks().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn init_config_file_loadable() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_project(dir.path()).unwrap();
        let config = store.load_config().unwrap();
        assert_eq!(config.statuses.len(), 5);
    }

    #[test]
    fn init_gitignore_entries() {
        let dir = tempfile::tempdir().unwrap();
        init_project(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains(".tc/"));
        assert!(content.contains(".tc-worktrees/"));
    }

    #[test]
    fn init_existing_gitignore_preserved() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();
        init_project(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(content.contains("node_modules/"));
        assert!(content.contains(".tc/"));
    }

    #[test]
    fn init_already_initialized() {
        let dir = tempfile::tempdir().unwrap();
        init_project(dir.path()).unwrap();
        let err = init_project(dir.path()).unwrap_err();
        assert!(matches!(err, StorageError::AlreadyInitialized(_)));
    }

    #[test]
    fn init_and_add_task_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_project(dir.path()).unwrap();

        let task = tc_core::task::Task {
            id: tc_core::task::TaskId("T-001".into()),
            title: "Test task".into(),
            epic: "backend".into(),
            status: tc_core::status::StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec!["src/main.rs".into()],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec!["Works".into()],
            assignee: None,
            created_at: chrono::Utc::now(),
        };

        store.save_tasks(&[task]).unwrap();
        let loaded = store.load_tasks().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id.0, "T-001");
        assert_eq!(loaded[0].acceptance_criteria, vec!["Works"]);
    }

    #[test]
    fn init_logs_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        init_project(dir.path()).unwrap();
        assert!(dir.path().join(".tc/logs").is_dir());
    }
}
