use std::collections::HashMap;

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};

use crate::error::CoreError;
use crate::status::StatusMachine;
use crate::task::{Task, TaskId};

#[derive(Debug)]
pub struct TaskDag {
    graph: DiGraph<TaskId, ()>,
    index: HashMap<TaskId, NodeIndex>,
}

impl TaskDag {
    pub fn from_tasks(tasks: &[Task]) -> Result<Self, CoreError> {
        let mut graph = DiGraph::new();
        let mut index = HashMap::new();

        for task in tasks {
            let node = graph.add_node(task.id.clone());
            index.insert(task.id.clone(), node);
        }

        for task in tasks {
            let to = index[&task.id];
            for dep_id in &task.depends_on {
                let from = index
                    .get(dep_id)
                    .ok_or_else(|| CoreError::orphan_dep(&task.id.0, &dep_id.0))?;
                graph.add_edge(*from, to, ());
            }
        }

        let dag = Self { graph, index };
        dag.validate()?;
        Ok(dag)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        toposort(&self.graph, None).map_err(|cycle| {
            let task_id = &self.graph[cycle.node_id()];
            CoreError::cycle(&task_id.0)
        })?;
        Ok(())
    }

    pub fn topological_order(&self) -> Result<Vec<TaskId>, CoreError> {
        let sorted = toposort(&self.graph, None).map_err(|cycle| {
            let task_id = &self.graph[cycle.node_id()];
            CoreError::cycle(&task_id.0)
        })?;
        Ok(sorted
            .into_iter()
            .map(|idx| self.graph[idx].clone())
            .collect())
    }

    pub fn compute_ready(&self, tasks: &[Task], sm: &StatusMachine) -> Vec<TaskId> {
        let task_map: HashMap<&TaskId, &Task> = tasks.iter().map(|t| (&t.id, t)).collect();

        tasks
            .iter()
            .filter(|t| !sm.is_terminal(&t.status))
            .filter(|t| !sm.is_active(&t.status))
            .filter(|t| {
                t.depends_on
                    .iter()
                    .all(|dep| task_map.get(dep).is_some_and(|d| sm.is_terminal(&d.status)))
            })
            .map(|t| t.id.clone())
            .collect()
    }

    pub fn dependencies(&self, id: &TaskId) -> Vec<TaskId> {
        let Some(&node) = self.index.get(id) else {
            return vec![];
        };
        self.graph
            .neighbors_directed(node, Direction::Incoming)
            .map(|idx| self.graph[idx].clone())
            .collect()
    }

    pub fn dependents(&self, id: &TaskId) -> Vec<TaskId> {
        let Some(&node) = self.index.get(id) else {
            return vec![];
        };
        self.graph
            .neighbors_directed(node, Direction::Outgoing)
            .map(|idx| self.graph[idx].clone())
            .collect()
    }

    pub fn unblocked_by(&self, id: &TaskId, tasks: &[Task], sm: &StatusMachine) -> Vec<TaskId> {
        let task_map: HashMap<&TaskId, &Task> = tasks.iter().map(|t| (&t.id, t)).collect();

        self.dependents(id)
            .into_iter()
            .filter(|dep_id| {
                task_map.get(dep_id).is_some_and(|t| {
                    t.depends_on
                        .iter()
                        .all(|d| task_map.get(d).is_some_and(|dt| sm.is_terminal(&dt.status)))
                })
            })
            .collect()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{StatusDef, StatusId};
    use chrono::Utc;

    fn make_task(id: &str, deps: &[&str], status: &str) -> Task {
        Task {
            id: TaskId(id.to_string()),
            title: format!("Task {id}"),
            epic: "test".to_string(),
            status: StatusId(status.to_string()),
            priority: crate::task::Priority::default(),
            tags: vec![],
            due: None,
            scheduled: None,
            estimate: None,
            depends_on: deps.iter().map(|d| TaskId(d.to_string())).collect(),
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        }
    }

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
    fn from_tasks_empty() {
        let dag = TaskDag::from_tasks(&[]).unwrap();
        assert_eq!(dag.node_count(), 0);
    }

    #[test]
    fn from_tasks_single() {
        let tasks = vec![make_task("T-001", &[], "todo")];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert_eq!(dag.node_count(), 1);
        assert_eq!(dag.edge_count(), 0);
    }

    #[test]
    fn from_tasks_with_deps() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
            make_task("T-003", &["T-001", "T-002"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert_eq!(dag.node_count(), 3);
        assert_eq!(dag.edge_count(), 3);
    }

    #[test]
    fn cycle_detection() {
        let tasks = vec![
            make_task("T-001", &["T-002"], "todo"),
            make_task("T-002", &["T-001"], "todo"),
        ];
        let err = TaskDag::from_tasks(&tasks).unwrap_err();
        assert!(matches!(err, CoreError::CycleDetected { .. }));
    }

    #[test]
    fn orphan_dependency() {
        let tasks = vec![make_task("T-001", &["T-999"], "todo")];
        let err = TaskDag::from_tasks(&tasks).unwrap_err();
        assert!(matches!(err, CoreError::OrphanDependency { .. }));
    }

    #[test]
    fn topological_order_linear() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
            make_task("T-003", &["T-002"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let order = dag.topological_order().unwrap();
        let ids: Vec<&str> = order.iter().map(|id| id.0.as_str()).collect();
        // T-001 must come before T-002, T-002 before T-003
        assert!(
            ids.iter().position(|&x| x == "T-001").unwrap()
                < ids.iter().position(|&x| x == "T-002").unwrap()
        );
        assert!(
            ids.iter().position(|&x| x == "T-002").unwrap()
                < ids.iter().position(|&x| x == "T-003").unwrap()
        );
    }

    #[test]
    fn compute_ready_all_todo() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let ready = dag.compute_ready(&tasks, &sm);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, "T-001");
    }

    #[test]
    fn compute_ready_after_done() {
        let tasks = vec![
            make_task("T-001", &[], "done"),
            make_task("T-002", &["T-001"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let ready = dag.compute_ready(&tasks, &sm);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, "T-002");
    }

    #[test]
    fn compute_ready_skips_in_progress() {
        let tasks = vec![make_task("T-001", &[], "in_progress")];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let ready = dag.compute_ready(&tasks, &sm);
        assert!(ready.is_empty());
    }

    #[test]
    fn compute_ready_skips_terminal() {
        let tasks = vec![make_task("T-001", &[], "done")];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let ready = dag.compute_ready(&tasks, &sm);
        assert!(ready.is_empty());
    }

    #[test]
    fn dependencies_and_dependents() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let deps = dag.dependencies(&TaskId("T-002".into()));
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].0, "T-001");

        let dependents = dag.dependents(&TaskId("T-001".into()));
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].0, "T-002");
    }

    #[test]
    fn dependencies_of_unknown_returns_empty() {
        let tasks = vec![make_task("T-001", &[], "todo")];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert!(dag.dependencies(&TaskId("T-999".into())).is_empty());
        assert!(dag.dependents(&TaskId("T-999".into())).is_empty());
    }

    #[test]
    fn unblocked_by_marks_downstream_ready() {
        let tasks = vec![
            make_task("T-001", &[], "done"),
            make_task("T-002", &["T-001"], "todo"),
            make_task("T-003", &["T-001"], "blocked"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let unblocked = dag.unblocked_by(&TaskId("T-001".into()), &tasks, &sm);
        assert_eq!(unblocked.len(), 2);
    }

    #[test]
    fn unblocked_by_partial_deps() {
        // T-003 depends on both T-001 and T-002; only T-001 is done
        let tasks = vec![
            make_task("T-001", &[], "done"),
            make_task("T-002", &[], "todo"),
            make_task("T-003", &["T-001", "T-002"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let unblocked = dag.unblocked_by(&TaskId("T-001".into()), &tasks, &sm);
        // T-003 is NOT unblocked because T-002 is still todo
        assert!(unblocked.is_empty());
    }

    #[test]
    fn from_tasks_multiple_independent() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &[], "todo"),
            make_task("T-003", &[], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert_eq!(dag.node_count(), 3);
        assert_eq!(dag.edge_count(), 0);
    }

    #[test]
    fn topological_order_diamond() {
        // Diamond: T-001 -> T-002, T-001 -> T-003, T-002 -> T-004, T-003 -> T-004
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
            make_task("T-003", &["T-001"], "todo"),
            make_task("T-004", &["T-002", "T-003"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let order = dag.topological_order().unwrap();
        let pos = |id: &str| order.iter().position(|x| x.0 == id).unwrap();
        assert!(pos("T-001") < pos("T-002"));
        assert!(pos("T-001") < pos("T-003"));
        assert!(pos("T-002") < pos("T-004"));
        assert!(pos("T-003") < pos("T-004"));
    }

    #[test]
    fn compute_ready_multiple_roots() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &[], "todo"),
            make_task("T-003", &["T-001", "T-002"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let ready = dag.compute_ready(&tasks, &sm);
        assert_eq!(ready.len(), 2);
        let ids: Vec<&str> = ready.iter().map(|r| r.0.as_str()).collect();
        assert!(ids.contains(&"T-001"));
        assert!(ids.contains(&"T-002"));
    }

    #[test]
    fn compute_ready_blocked_not_ready() {
        let tasks = vec![make_task("T-001", &[], "blocked")];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let ready = dag.compute_ready(&tasks, &sm);
        // blocked is non-terminal, not in_progress, and has no deps -> it IS ready
        assert_eq!(ready.len(), 1);
    }

    #[test]
    fn compute_ready_empty() {
        let dag = TaskDag::from_tasks(&[]).unwrap();
        let sm = default_sm();
        let ready = dag.compute_ready(&[], &sm);
        assert!(ready.is_empty());
    }

    #[test]
    fn cycle_detection_three_node() {
        let tasks = vec![
            make_task("T-001", &["T-003"], "todo"),
            make_task("T-002", &["T-001"], "todo"),
            make_task("T-003", &["T-002"], "todo"),
        ];
        let err = TaskDag::from_tasks(&tasks).unwrap_err();
        assert!(matches!(err, CoreError::CycleDetected { .. }));
    }

    #[test]
    fn dependents_of_leaf_is_empty() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert!(dag.dependents(&TaskId("T-002".into())).is_empty());
    }

    #[test]
    fn dependencies_of_root_is_empty() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert!(dag.dependencies(&TaskId("T-001".into())).is_empty());
    }

    #[test]
    fn node_count_and_edge_count() {
        let tasks = vec![
            make_task("T-001", &[], "todo"),
            make_task("T-002", &["T-001"], "todo"),
            make_task("T-003", &["T-001"], "todo"),
        ];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        assert_eq!(dag.node_count(), 3);
        assert_eq!(dag.edge_count(), 2);
    }

    #[test]
    fn unblocked_by_unknown_returns_empty() {
        let tasks = vec![make_task("T-001", &[], "done")];
        let dag = TaskDag::from_tasks(&tasks).unwrap();
        let sm = default_sm();
        let unblocked = dag.unblocked_by(&TaskId("T-999".into()), &tasks, &sm);
        assert!(unblocked.is_empty());
    }
}
