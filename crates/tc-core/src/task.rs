use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::status::StatusId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Assignee {
    Claude,
    Opencode,
    Human,
}

/// Task priority -- surfaced in UIs and used as a sort key.
///
/// Derived [`Ord`] follows enum declaration order, so the natural order is
/// `P1 < P2 < P3 < P4 < P5`. An **ascending** sort
/// (e.g. `tasks.sort_by_key(|t| t.priority)`) puts the most important
/// tasks *first* -- handy for UI lists, but surprising if you expect
/// "higher priority = larger value". Compare explicitly when that
/// intuition matters.
///
/// Legacy YAML using `critical` / `high` / `normal` / `low` continues to
/// load via serde aliases (introduced in M-6.3). `tc migrate` rewrites to
/// the canonical `p1`..`p5` names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    #[serde(alias = "critical")]
    P1,
    #[serde(alias = "high")]
    P2,
    #[default]
    #[serde(alias = "normal")]
    P3,
    #[serde(alias = "low")]
    P4,
    P5,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::P1 => f.write_str("p1"),
            Self::P2 => f.write_str("p2"),
            Self::P3 => f.write_str("p3"),
            Self::P4 => f.write_str("p4"),
            Self::P5 => f.write_str("p5"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub epic: String,
    pub status: StatusId,
    #[serde(default)]
    pub priority: Priority,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<NaiveDate>,
    #[serde(
        default,
        with = "humantime_serde",
        skip_serializing_if = "Option::is_none"
    )]
    pub estimate: Option<Duration>,
    #[serde(default)]
    pub depends_on: Vec<TaskId>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub pack_exclude: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub assignee: Option<Assignee>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_yaml() -> &'static str {
        // Pre-M-6.1 shape: no priority/tags/due/scheduled/estimate fields.
        // Loading this must succeed -- this is the non-breaking guarantee
        // upstream code (tc-storage) relies on for in-place YAML migration.
        "\
id: T-001
title: Old task
epic: legacy
status: todo
assignee: null
created_at: 2026-01-01T00:00:00Z
"
    }

    #[test]
    fn legacy_yaml_loads_with_field_defaults() {
        let task: Task = serde_yaml_ng::from_str(minimal_yaml()).expect("legacy yaml must load");
        assert_eq!(task.priority, Priority::default());
        assert!(task.tags.is_empty());
        assert!(task.due.is_none());
        assert!(task.scheduled.is_none());
        assert!(task.estimate.is_none());
    }

    #[test]
    fn empty_extension_fields_are_omitted_from_yaml() {
        let task = Task {
            id: TaskId("T-100".into()),
            title: "no extras".into(),
            epic: "default".into(),
            status: StatusId("todo".into()),
            priority: Priority::default(),
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
            created_at: "2026-04-25T00:00:00Z".parse().unwrap(),
        };
        let yaml = serde_yaml_ng::to_string(&task).unwrap();
        assert!(!yaml.contains("tags:"), "yaml = {yaml}");
        assert!(!yaml.contains("due:"), "yaml = {yaml}");
        assert!(!yaml.contains("scheduled:"), "yaml = {yaml}");
        assert!(!yaml.contains("estimate:"), "yaml = {yaml}");
    }

    #[test]
    fn extension_fields_round_trip_through_yaml() {
        let task = Task {
            id: TaskId("T-101".into()),
            title: "with extras".into(),
            epic: "default".into(),
            status: StatusId("todo".into()),
            priority: Priority::P2,
            tags: vec!["backend".into(), "perf".into()],
            due: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            scheduled: Some(NaiveDate::from_ymd_opt(2026, 4, 28).unwrap()),
            estimate: Some(Duration::from_secs(2 * 3600 + 30 * 60)),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: "2026-04-25T00:00:00Z".parse().unwrap(),
        };

        let yaml = serde_yaml_ng::to_string(&task).unwrap();
        let decoded: Task = serde_yaml_ng::from_str(&yaml).unwrap();

        assert_eq!(decoded.tags, vec!["backend".to_string(), "perf".into()]);
        assert_eq!(decoded.due, task.due);
        assert_eq!(decoded.scheduled, task.scheduled);
        assert_eq!(decoded.estimate, task.estimate);
        assert_eq!(decoded.priority, Priority::P2);
    }

    #[test]
    fn legacy_priority_names_load_via_aliases() {
        // Pre-M-6.3 YAML used critical/high/normal/low. Each must still
        // deserialize into the equivalent P1..P4 variant -- the migration
        // contract from M-6.2 promised non-breaking loads.
        let cases = [
            ("critical", Priority::P1),
            ("high", Priority::P2),
            ("normal", Priority::P3),
            ("low", Priority::P4),
        ];
        for (legacy, expected) in cases {
            let yaml = format!(
                "id: T-200\ntitle: legacy priority\nepic: legacy\nstatus: todo\nassignee: null\ncreated_at: 2026-04-25T00:00:00Z\npriority: {legacy}\n"
            );
            let task: Task = serde_yaml_ng::from_str(&yaml)
                .unwrap_or_else(|e| panic!("legacy '{legacy}' must load: {e}"));
            assert_eq!(task.priority, expected, "alias '{legacy}'");
        }
    }

    #[test]
    fn priority_round_trip_uses_canonical_p_names() {
        // Loading a legacy alias and re-serializing must produce the new
        // canonical name -- this is what `tc migrate` relies on.
        let yaml_in = "id: T-201\ntitle: legacy alias\nepic: legacy\nstatus: todo\nassignee: null\ncreated_at: 2026-04-25T00:00:00Z\npriority: critical\n";
        let task: Task = serde_yaml_ng::from_str(yaml_in).unwrap();
        let yaml_out = serde_yaml_ng::to_string(&task).unwrap();
        assert!(
            yaml_out.contains("priority: p1"),
            "expected canonical p1 in re-serialized YAML, got:\n{yaml_out}"
        );
        assert!(
            !yaml_out.contains("priority: critical"),
            "alias name must not survive round-trip:\n{yaml_out}"
        );
    }

    #[test]
    fn priority_ord_puts_p1_first() {
        let mut p = vec![Priority::P3, Priority::P5, Priority::P1, Priority::P2];
        p.sort();
        assert_eq!(
            p,
            vec![Priority::P1, Priority::P2, Priority::P3, Priority::P5]
        );
    }

    #[test]
    fn estimate_accepts_humantime_string() {
        let yaml = "\
id: T-102
title: with estimate
epic: default
status: todo
assignee: null
created_at: 2026-04-25T00:00:00Z
estimate: 2h 30m
";
        let task: Task = serde_yaml_ng::from_str(yaml).expect("humantime estimate must parse");
        assert_eq!(task.estimate, Some(Duration::from_secs(2 * 3600 + 30 * 60)));
    }

    #[test]
    fn due_and_scheduled_accept_iso_dates() {
        let yaml = "\
id: T-103
title: dated
epic: default
status: todo
assignee: null
created_at: 2026-04-25T00:00:00Z
due: 2026-05-15
scheduled: 2026-05-10
";
        let task: Task = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(task.due, NaiveDate::from_ymd_opt(2026, 5, 15));
        assert_eq!(task.scheduled, NaiveDate::from_ymd_opt(2026, 5, 10));
    }
}
