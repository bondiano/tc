use minijinja::Environment;

use crate::dag::TaskDag;
use crate::error::CoreError;
use crate::status::StatusMachine;
use crate::task::{Task, TaskId};

/// Render dependencies for `task_id` as a human-readable comma-separated list
/// like `"T-002 Title [done], T-003 Other [pending]"`. Returns `"none"` when
/// the task has no deps.
pub fn build_resolved_deps(
    tasks: &[Task],
    dag: &TaskDag,
    task_id: &TaskId,
    sm: &StatusMachine,
) -> String {
    let deps = dag.dependencies(task_id);
    if deps.is_empty() {
        return "none".to_string();
    }
    deps.iter()
        .filter_map(|dep_id| {
            tasks.iter().find(|t| t.id == *dep_id).map(|t| {
                let status_marker = if sm.is_terminal(&t.status) {
                    "[done]"
                } else {
                    "[pending]"
                };
                format!("{} {} {status_marker}", t.id, t.title)
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub struct ContextRenderer {
    env: Environment<'static>,
}

impl ContextRenderer {
    pub fn new(template: &str) -> Result<Self, CoreError> {
        let mut env = Environment::new();
        env.add_template_owned("context", template.to_owned())
            .map_err(CoreError::template)?;
        Ok(Self { env })
    }

    pub fn render(
        &self,
        task: &Task,
        resolved_deps: &str,
        packed_files: Option<&str>,
    ) -> Result<String, CoreError> {
        let tmpl = self
            .env
            .get_template("context")
            .map_err(CoreError::template)?;

        let acceptance_criteria = if task.acceptance_criteria.is_empty() {
            None
        } else {
            Some(
                task.acceptance_criteria
                    .iter()
                    .map(|ac| format!("- {ac}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        };

        let ctx = minijinja::context! {
            id => task.id.0,
            title => task.title,
            epic => task.epic,
            priority => task.priority.to_string(),
            resolved_deps => resolved_deps,
            notes => task.notes,
            packed_files => packed_files,
            acceptance_criteria => acceptance_criteria,
        };

        tmpl.render(ctx).map_err(CoreError::template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::StatusId;
    use crate::task::TaskId;
    use chrono::Utc;

    fn make_task() -> Task {
        Task {
            id: TaskId("T-001".into()),
            title: "Test task".into(),
            epic: "backend".into(),
            status: StatusId("todo".into()),
            priority: crate::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: "Some notes here".into(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn render_basic_template() {
        let renderer = ContextRenderer::new("Task: {{ id }} - {{ title }}").unwrap();
        let result = renderer.render(&make_task(), "none", None).unwrap();
        assert_eq!(result, "Task: T-001 - Test task");
    }

    #[test]
    fn render_with_resolved_deps() {
        let renderer = ContextRenderer::new("Deps: {{ resolved_deps }}").unwrap();
        let result = renderer.render(&make_task(), "T-002, T-003", None).unwrap();
        assert_eq!(result, "Deps: T-002, T-003");
    }

    #[test]
    fn render_with_packed_files() {
        let template = "{% if packed_files %}Files: {{ packed_files }}{% endif %}";
        let renderer = ContextRenderer::new(template).unwrap();
        let result = renderer
            .render(&make_task(), "none", Some("file1.rs"))
            .unwrap();
        assert_eq!(result, "Files: file1.rs");
    }

    #[test]
    fn render_without_packed_files() {
        let template =
            "{% if packed_files %}Files: {{ packed_files }}{% else %}No files{% endif %}";
        let renderer = ContextRenderer::new(template).unwrap();
        let result = renderer.render(&make_task(), "none", None).unwrap();
        assert_eq!(result, "No files");
    }

    #[test]
    fn render_notes() {
        let renderer = ContextRenderer::new("Notes: {{ notes }}").unwrap();
        let result = renderer.render(&make_task(), "", None).unwrap();
        assert_eq!(result, "Notes: Some notes here");
    }

    #[test]
    fn render_epic() {
        let renderer = ContextRenderer::new("Epic: {{ epic }}").unwrap();
        let result = renderer.render(&make_task(), "", None).unwrap();
        assert_eq!(result, "Epic: backend");
    }

    #[test]
    fn invalid_template_syntax() {
        let result = ContextRenderer::new("{{ unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn render_acceptance_criteria() {
        let template =
            "{% if acceptance_criteria %}AC:\n{{ acceptance_criteria }}{% else %}No AC{% endif %}";
        let renderer = ContextRenderer::new(template).unwrap();
        let mut task = make_task();
        task.acceptance_criteria = vec!["API returns 200".into(), "Tests pass".into()];
        let result = renderer.render(&task, "", None).unwrap();
        assert!(result.contains("API returns 200"));
        assert!(result.contains("Tests pass"));
    }

    #[test]
    fn render_acceptance_criteria_empty() {
        let template =
            "{% if acceptance_criteria %}AC: {{ acceptance_criteria }}{% else %}No AC{% endif %}";
        let renderer = ContextRenderer::new(template).unwrap();
        let result = renderer.render(&make_task(), "", None).unwrap();
        assert_eq!(result, "No AC");
    }

    #[test]
    fn render_full_template() {
        let template = "# Task {{ id }}: {{ title }}\n**Epic:** {{ epic }}\n**Deps:** {{ resolved_deps }}\n## Notes\n{{ notes }}";
        let renderer = ContextRenderer::new(template).unwrap();
        let result = renderer.render(&make_task(), "T-002, T-003", None).unwrap();
        assert!(result.contains("# Task T-001: Test task"));
        assert!(result.contains("**Epic:** backend"));
        assert!(result.contains("**Deps:** T-002, T-003"));
        assert!(result.contains("Some notes here"));
    }

    #[test]
    fn render_empty_notes() {
        let renderer = ContextRenderer::new("Notes: [{{ notes }}]").unwrap();
        let mut task = make_task();
        task.notes = String::new();
        let result = renderer.render(&task, "", None).unwrap();
        assert_eq!(result, "Notes: []");
    }
}
