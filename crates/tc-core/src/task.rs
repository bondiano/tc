use chrono::{DateTime, Utc};
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Critical,
    High,
    #[default]
    Normal,
    Low,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => f.write_str("critical"),
            Self::High => f.write_str("high"),
            Self::Normal => f.write_str("normal"),
            Self::Low => f.write_str("low"),
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
