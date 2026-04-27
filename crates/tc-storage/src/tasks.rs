use std::path::Path;

use tc_core::task::{Task, TaskId};

use crate::atomic::write_atomic;
use crate::error::{StorageError, StorageResult};

#[derive(serde::Serialize, serde::Deserialize)]
struct TasksFile {
    #[serde(default)]
    tasks: Vec<Task>,
}

/// Outcome of a YAML normalization pass.
///
/// `changed` is `true` when the on-disk bytes would differ from the bytes we
/// produce after a load + re-serialize round-trip. That happens whenever the
/// file is missing fields that the current `Task` schema serializes
/// unconditionally (e.g. legacy files lacking `priority:`), or whenever
/// formatting drift exists (key order, whitespace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub tasks_loaded: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub changed: bool,
}

/// Compute what migrating `path` would produce, without writing.
///
/// Returns `(report, normalized_yaml)`. Callers may inspect the report,
/// print a diff using the YAML, or pass the YAML to [`write_atomic`].
pub fn plan_migration(path: &Path) -> StorageResult<(MigrationReport, String)> {
    let original = std::fs::read_to_string(path).map_err(|e| StorageError::file_read(path, e))?;
    let file: TasksFile =
        serde_yaml_ng::from_str(&original).map_err(|e| StorageError::yaml_parse(path, e))?;

    let normalized = serde_yaml_ng::to_string(&file).map_err(StorageError::YamlSerialize)?;
    let report = MigrationReport {
        tasks_loaded: file.tasks.len(),
        bytes_before: original.len(),
        bytes_after: normalized.len(),
        changed: original != normalized,
    };
    Ok((report, normalized))
}

/// Atomically rewrite `path` to its normalized form. No-op-safe: even when
/// the bytes are identical, callers can still call this without harm.
pub fn migrate(path: &Path) -> StorageResult<MigrationReport> {
    let (report, normalized) = plan_migration(path)?;
    if report.changed {
        write_atomic(path, normalized.as_bytes())?;
    }
    Ok(report)
}

pub fn load(path: &Path) -> StorageResult<Vec<Task>> {
    let content = std::fs::read_to_string(path).map_err(|e| StorageError::file_read(path, e))?;
    let file: TasksFile =
        serde_yaml_ng::from_str(&content).map_err(|e| StorageError::yaml_parse(path, e))?;
    Ok(file.tasks)
}

pub fn save(path: &Path, tasks: &[Task]) -> StorageResult<()> {
    let file = TasksFile {
        tasks: tasks.to_vec(),
    };
    let content = serde_yaml_ng::to_string(&file).map_err(StorageError::YamlSerialize)?;
    write_atomic(path, content.as_bytes())?;
    Ok(())
}

pub fn next_id(tasks: &[Task]) -> TaskId {
    // Numeric IDs already in use. Non-`T-NNN` ids are invisible to this
    // set -- they're fine because we only ever mint `T-NNN` ids ourselves,
    // but we also dodge collisions with them below.
    let used_numeric: std::collections::HashSet<u32> = tasks
        .iter()
        .filter_map(|t| {
            t.id.0
                .strip_prefix("T-")
                .and_then(|n| n.parse::<u32>().ok())
        })
        .collect();

    let used_full: std::collections::HashSet<&str> =
        tasks.iter().map(|t| t.id.0.as_str()).collect();

    let start = used_numeric.iter().copied().max().unwrap_or(0) + 1;
    // Safety upper bound: prevent pathological infinite loops on corrupt input.
    const MAX_SCAN: u32 = 1_000_000;

    let mut n = start;
    'scan: loop {
        let candidate = format!("T-{n:03}");
        if !used_numeric.contains(&n) && !used_full.contains(candidate.as_str()) {
            return TaskId(candidate);
        }
        n += 1;
        if n - start > MAX_SCAN {
            // Fall back to a numeric-only ID far past the max; callers
            // would hit this only with truly broken state.
            return TaskId(format!("T-{n:03}"));
        }
        continue 'scan;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tc_core::status::StatusId;

    fn make_task(id: &str) -> Task {
        Task {
            id: TaskId(id.to_string()),
            title: format!("Task {id}"),
            epic: "test".to_string(),
            status: StatusId("todo".to_string()),
            priority: tc_core::task::Priority::default(),
            tags: vec![],
            due: None,
            scheduled: None,
            estimate: None,
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        let tasks = vec![make_task("T-001"), make_task("T-002")];

        save(&path, &tasks).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id.0, "T-001");
        assert_eq!(loaded[1].id.0, "T-002");
    }

    #[test]
    fn roundtrip_with_deps_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        let mut task = make_task("T-002");
        task.depends_on = vec![TaskId("T-001".into())];
        task.files = vec!["src/main.rs".into(), "src/lib.rs".into()];
        task.notes = "Some notes".into();

        save(&path, &[task]).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded[0].depends_on.len(), 1);
        assert_eq!(loaded[0].depends_on[0].0, "T-001");
        assert_eq!(loaded[0].files.len(), 2);
        assert_eq!(loaded[0].notes, "Some notes");
    }

    #[test]
    fn roundtrip_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");

        save(&path, &[]).unwrap();
        let loaded = load(&path).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn next_id_empty() {
        assert_eq!(next_id(&[]).0, "T-001");
    }

    #[test]
    fn next_id_sequential() {
        let tasks = vec![make_task("T-001"), make_task("T-002")];
        assert_eq!(next_id(&tasks).0, "T-003");
    }

    #[test]
    fn next_id_with_gap() {
        let tasks = vec![make_task("T-001"), make_task("T-005")];
        assert_eq!(next_id(&tasks).0, "T-006");
    }

    #[test]
    fn load_nonexistent_file() {
        let result = load(std::path::Path::new("/nonexistent/tasks.yaml"));
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_with_acceptance_criteria() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        let mut task = make_task("T-001");
        task.acceptance_criteria = vec!["API returns 200".into(), "Tests pass".into()];

        save(&path, &[task]).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded[0].acceptance_criteria.len(), 2);
        assert_eq!(loaded[0].acceptance_criteria[0], "API returns 200");
        assert_eq!(loaded[0].acceptance_criteria[1], "Tests pass");
    }

    #[test]
    fn roundtrip_with_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        let mut task = make_task("T-003");
        task.depends_on = vec![TaskId("T-001".into()), TaskId("T-002".into())];
        task.files = vec!["src/main.rs".into()];
        task.pack_exclude = vec!["*.test.*".into()];
        task.notes = "Important notes".into();
        task.acceptance_criteria = vec!["Works".into()];
        task.assignee = Some(tc_core::task::Assignee::Claude);
        task.priority = tc_core::task::Priority::P1;

        save(&path, &[task]).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded[0].id.0, "T-003");
        assert_eq!(loaded[0].depends_on.len(), 2);
        assert_eq!(loaded[0].files, vec!["src/main.rs"]);
        assert_eq!(loaded[0].pack_exclude, vec!["*.test.*"]);
        assert_eq!(loaded[0].notes, "Important notes");
        assert_eq!(loaded[0].acceptance_criteria, vec!["Works"]);
        assert!(loaded[0].assignee.is_some());
        assert_eq!(loaded[0].priority, tc_core::task::Priority::P1);
    }

    #[test]
    fn roundtrip_with_priority() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        let mut task = make_task("T-001");
        task.priority = tc_core::task::Priority::P2;

        save(&path, &[task]).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded[0].priority, tc_core::task::Priority::P2);
    }

    #[test]
    fn roundtrip_priority_defaults_to_p3() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        // Write YAML without priority field to test backward compatibility
        std::fs::write(&path, "tasks:\n- id: T-001\n  title: Test\n  epic: be\n  status: todo\n  assignee: null\n  created_at: 2025-01-01T00:00:00Z\n").unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded[0].priority, tc_core::task::Priority::P3);
    }

    #[test]
    fn roundtrip_preserves_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        let tasks = vec![make_task("T-003"), make_task("T-001"), make_task("T-002")];

        save(&path, &tasks).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded[0].id.0, "T-003");
        assert_eq!(loaded[1].id.0, "T-001");
        assert_eq!(loaded[2].id.0, "T-002");
    }

    #[test]
    fn next_id_with_non_standard_ids() {
        let mut task = make_task("custom-id");
        task.id = TaskId("custom-id".into());
        let tasks = vec![task, make_task("T-010")];
        assert_eq!(next_id(&tasks).0, "T-011");
    }

    #[test]
    fn next_id_single_task() {
        let tasks = vec![make_task("T-042")];
        assert_eq!(next_id(&tasks).0, "T-043");
    }

    #[test]
    fn next_id_skips_already_used_numeric() {
        // Manually crafted case: max numeric is 002, but someone inserted
        // T-003 with a lowercase prefix / non-standard form that we parse
        // back. Here we simulate the equivalent pathology by reserving
        // T-003 directly.
        let tasks = vec![make_task("T-001"), make_task("T-002"), make_task("T-003")];
        assert_eq!(next_id(&tasks).0, "T-004");
    }

    const LEGACY_TASKS_YAML: &str = "tasks:\n- id: T-100\n  title: Pre-6.1 task\n  epic: legacy\n  status: todo\n  assignee: null\n  created_at: 2026-01-01T00:00:00Z\n";

    #[test]
    fn legacy_yaml_loads_with_extension_field_defaults() {
        // A file written before M-6.1 (no priority/tags/due/scheduled/estimate)
        // must continue to load -- this is the migration contract M-6.2 commits to.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        std::fs::write(&path, LEGACY_TASKS_YAML).unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        let t = &loaded[0];
        assert_eq!(t.priority, tc_core::task::Priority::P3);
        assert!(t.tags.is_empty());
        assert!(t.due.is_none());
        assert!(t.scheduled.is_none());
        assert!(t.estimate.is_none());
    }

    #[test]
    fn migrate_legacy_file_marks_changed_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        std::fs::write(&path, LEGACY_TASKS_YAML).unwrap();

        let report = migrate(&path).unwrap();
        assert_eq!(report.tasks_loaded, 1);
        assert!(
            report.changed,
            "legacy file lacks priority and must be rewritten"
        );

        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("priority: p3"),
            "rewrite should populate priority explicitly: {written}"
        );
        // Empty extension fields must stay omitted to keep diffs small.
        assert!(!written.contains("tags:"), "empty tags must not be emitted");
        assert!(!written.contains("due:"), "missing due must not be emitted");
        assert!(
            !written.contains("estimate:"),
            "missing estimate must not be emitted"
        );
    }

    #[test]
    fn migrate_already_normalized_file_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        let tasks = vec![make_task("T-001"), make_task("T-002")];
        save(&path, &tasks).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();

        let report = migrate(&path).unwrap();
        assert_eq!(report.tasks_loaded, 2);
        assert!(
            !report.changed,
            "saved-then-migrated file should be byte-identical"
        );

        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn plan_migration_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.yaml");
        std::fs::write(&path, LEGACY_TASKS_YAML).unwrap();

        let (report, normalized) = plan_migration(&path).unwrap();
        assert!(report.changed);
        assert!(normalized.contains("priority: p3"));

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(on_disk, LEGACY_TASKS_YAML, "plan_migration must not write");
    }

    #[test]
    fn next_id_skips_custom_id_collision() {
        // max numeric is 002, so new candidate would be T-003 -- but a task
        // with literal id "T-003" exists (even though it'd normally be
        // caught above, exercise the `used_full` path explicitly).
        let mut t = make_task("manual");
        t.id = TaskId("T-003".into());
        let tasks = vec![make_task("T-001"), make_task("T-002"), t];
        assert_eq!(next_id(&tasks).0, "T-004");
    }
}
