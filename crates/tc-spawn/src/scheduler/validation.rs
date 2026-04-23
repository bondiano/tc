use std::collections::HashMap;

use tc_core::task::Task;

use crate::error::SpawnError;

/// Detect file conflicts between tasks being spawned.
///
/// Two tasks conflict when they list the same path in their `files` field;
/// spawning them in parallel risks clobbering edits, so the caller must
/// abort before any worktree is created.
pub fn detect_file_conflicts(tasks: &[&Task]) -> Result<(), SpawnError> {
    let mut file_owners: HashMap<&str, Vec<&str>> = HashMap::new();
    for task in tasks {
        for file in &task.files {
            file_owners
                .entry(file.as_str())
                .or_default()
                .push(&task.id.0);
        }
    }
    if let Some((path, owners)) = file_owners.iter().find(|(_, o)| o.len() > 1) {
        return Err(SpawnError::file_conflict(
            &owners.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
            *path,
        ));
    }
    Ok(())
}

/// Validate that a queue of tasks can be safely run together.
///
/// Intended as a pre-flight check on the full queue before spawning
/// any workers -- currently detects file conflicts across the queue.
pub fn validate_queue(tasks: &[&Task]) -> Result<(), SpawnError> {
    detect_file_conflicts(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Utc;
    use tc_core::status::StatusId;
    use tc_core::task::TaskId;

    fn make_task(id: &str, files: Vec<String>) -> Task {
        Task {
            id: TaskId(id.into()),
            title: format!("Task {id}"),
            epic: "test".into(),
            status: StatusId("todo".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files,
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn detect_no_file_conflicts() {
        let t1 = make_task("T-001", vec!["src/a.rs".into()]);
        let t2 = make_task("T-002", vec!["src/b.rs".into()]);
        let tasks: Vec<&Task> = vec![&t1, &t2];

        assert!(detect_file_conflicts(&tasks).is_ok());
    }

    #[test]
    fn detect_file_conflict() {
        let t1 = make_task("T-001", vec!["src/shared.rs".into()]);
        let t2 = make_task("T-002", vec!["src/shared.rs".into()]);
        let tasks: Vec<&Task> = vec![&t1, &t2];

        let err = detect_file_conflicts(&tasks).unwrap_err();
        assert!(matches!(err, SpawnError::FileConflict { .. }));
    }

    #[test]
    fn detect_no_conflict_with_empty_files() {
        let t1 = make_task("T-001", vec![]);
        let t2 = make_task("T-002", vec![]);
        let tasks: Vec<&Task> = vec![&t1, &t2];

        assert!(detect_file_conflicts(&tasks).is_ok());
    }

    #[test]
    fn validate_queue_wraps_file_conflicts() {
        let t1 = make_task("T-001", vec!["src/a.rs".into()]);
        let t2 = make_task("T-002", vec!["src/a.rs".into()]);
        let tasks: Vec<&Task> = vec![&t1, &t2];

        let err = validate_queue(&tasks).unwrap_err();
        assert!(matches!(err, SpawnError::FileConflict { .. }));
    }

    #[test]
    fn validate_queue_ok_when_no_conflicts() {
        let t1 = make_task("T-001", vec!["src/a.rs".into()]);
        let t2 = make_task("T-002", vec!["src/b.rs".into()]);
        let tasks: Vec<&Task> = vec![&t1, &t2];

        assert!(validate_queue(&tasks).is_ok());
    }
}
