use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatusId(pub String);

impl StatusId {
    pub fn todo() -> Self {
        Self("todo".into())
    }
    pub fn in_progress() -> Self {
        Self("in_progress".into())
    }
    pub fn done() -> Self {
        Self("done".into())
    }
    pub fn blocked() -> Self {
        Self("blocked".into())
    }
    pub fn review() -> Self {
        Self("review".into())
    }
}

impl std::fmt::Display for StatusId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDef {
    pub id: StatusId,
    pub label: String,
    pub terminal: bool,
    /// Marks a status as "work in progress". Active statuses are excluded
    /// from `ready` computation (a task that's already being worked on is
    /// not ready to be picked up again).
    ///
    /// Defaults to `false` for back-compat with configs that predate this
    /// field; see [`StatusMachine::new`] for the automatic migration that
    /// keeps the built-in `in_progress` status active.
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct StatusMachine {
    statuses: Vec<StatusDef>,
}

impl StatusMachine {
    pub fn new(mut statuses: Vec<StatusDef>) -> Self {
        // Back-compat migration: if nothing is marked active and the default
        // `in_progress` status is present, promote it. This keeps
        // `compute_ready` correct for configs written before `active` existed,
        // and stays a no-op for anyone who already set `active: true` on
        // their custom "wip" status.
        if !statuses.iter().any(|s| s.active)
            && let Some(s) = statuses
                .iter_mut()
                .find(|s| s.id == StatusId::in_progress())
        {
            s.active = true;
        }
        Self { statuses }
    }

    pub fn is_terminal(&self, id: &StatusId) -> bool {
        self.statuses
            .iter()
            .find(|s| s.id == *id)
            .is_some_and(|s| s.terminal)
    }

    /// Whether a status represents in-flight work. Active statuses are
    /// excluded from `ready` computation.
    pub fn is_active(&self, id: &StatusId) -> bool {
        self.statuses
            .iter()
            .find(|s| s.id == *id)
            .is_some_and(|s| s.active)
    }

    pub fn validate(&self, id: &StatusId) -> Result<(), CoreError> {
        if self.statuses.iter().any(|s| s.id == *id) {
            Ok(())
        } else {
            let valid: Vec<String> = self.statuses.iter().map(|s| s.id.0.clone()).collect();
            Err(CoreError::unknown_status(&id.0, &valid))
        }
    }

    pub fn statuses(&self) -> &[StatusDef] {
        &self.statuses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;

    fn default_sm() -> StatusMachine {
        StatusMachine::new(vec![
            StatusDef {
                id: StatusId("todo".into()),
                label: "Todo".into(),
                terminal: false,
                active: false,
            },
            StatusDef {
                id: StatusId("in_progress".into()),
                label: "In Progress".into(),
                terminal: false,
                active: false,
            },
            StatusDef {
                id: StatusId("done".into()),
                label: "Done".into(),
                terminal: true,
                active: false,
            },
            StatusDef {
                id: StatusId("blocked".into()),
                label: "Blocked".into(),
                terminal: false,
                active: false,
            },
        ])
    }

    #[test]
    fn is_terminal_done() {
        let sm = default_sm();
        assert!(sm.is_terminal(&StatusId("done".into())));
    }

    #[test]
    fn is_terminal_todo_false() {
        let sm = default_sm();
        assert!(!sm.is_terminal(&StatusId("todo".into())));
    }

    #[test]
    fn is_terminal_unknown_false() {
        let sm = default_sm();
        assert!(!sm.is_terminal(&StatusId("unknown".into())));
    }

    #[test]
    fn validate_known_status() {
        let sm = default_sm();
        assert!(sm.validate(&StatusId("todo".into())).is_ok());
        assert!(sm.validate(&StatusId("done".into())).is_ok());
    }

    #[test]
    fn validate_unknown_status() {
        let sm = default_sm();
        let err = sm.validate(&StatusId("wip".into())).unwrap_err();
        assert!(matches!(err, CoreError::UnknownStatus { .. }));
    }

    #[test]
    fn statuses_returns_all() {
        let sm = default_sm();
        assert_eq!(sm.statuses().len(), 4);
    }

    #[test]
    fn status_id_display() {
        let id = StatusId("todo".into());
        assert_eq!(id.to_string(), "todo");
    }

    #[test]
    fn is_terminal_blocked_false() {
        let sm = default_sm();
        assert!(!sm.is_terminal(&StatusId("blocked".into())));
    }

    #[test]
    fn is_terminal_in_progress_false() {
        let sm = default_sm();
        assert!(!sm.is_terminal(&StatusId("in_progress".into())));
    }

    #[test]
    fn validate_all_known_statuses() {
        let sm = default_sm();
        for status in &["todo", "in_progress", "done", "blocked"] {
            assert!(sm.validate(&StatusId((*status).into())).is_ok());
        }
    }

    #[test]
    fn validate_error_contains_valid_statuses() {
        let sm = default_sm();
        let err = sm.validate(&StatusId("wip".into())).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("wip"));
    }

    #[test]
    fn status_id_equality() {
        let a = StatusId("todo".into());
        let b = StatusId("todo".into());
        let c = StatusId("done".into());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn status_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(StatusId("todo".into()));
        set.insert(StatusId("todo".into()));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn empty_status_machine() {
        let sm = StatusMachine::new(vec![]);
        assert!(!sm.is_terminal(&StatusId("todo".into())));
        assert!(sm.validate(&StatusId("todo".into())).is_err());
        assert!(sm.statuses().is_empty());
    }

    #[test]
    fn legacy_config_auto_promotes_in_progress_active() {
        // Simulates loading a pre-`active` config from disk: all fields
        // default to `active: false`, but we expect `in_progress` to
        // behave as active for `compute_ready`.
        let sm = StatusMachine::new(vec![
            StatusDef {
                id: StatusId("todo".into()),
                label: "Todo".into(),
                terminal: false,
                active: false,
            },
            StatusDef {
                id: StatusId("in_progress".into()),
                label: "In Progress".into(),
                terminal: false,
                active: false,
            },
        ]);
        assert!(sm.is_active(&StatusId("in_progress".into())));
        assert!(!sm.is_active(&StatusId("todo".into())));
    }

    #[test]
    fn explicit_active_custom_status_is_respected() {
        // A user renamed in_progress to wip and marked it active. The
        // migration must NOT override their choice (and certainly must
        // not resurrect an in_progress status).
        let sm = StatusMachine::new(vec![
            StatusDef {
                id: StatusId("todo".into()),
                label: "Todo".into(),
                terminal: false,
                active: false,
            },
            StatusDef {
                id: StatusId("wip".into()),
                label: "WIP".into(),
                terminal: false,
                active: true,
            },
        ]);
        assert!(sm.is_active(&StatusId("wip".into())));
        assert!(!sm.is_active(&StatusId("todo".into())));
    }
}
