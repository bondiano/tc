use tc_core::task::{Assignee, Priority, TaskId};

use crate::editor::Editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateTaskField {
    Title,
    Epic,
    Priority,
    Assignee,
    DependsOn,
    Notes,
    AcceptanceCriteria,
    Files,
}

pub const FIELD_ORDER: &[CreateTaskField] = &[
    CreateTaskField::Title,
    CreateTaskField::Epic,
    CreateTaskField::Priority,
    CreateTaskField::Assignee,
    CreateTaskField::DependsOn,
    CreateTaskField::AcceptanceCriteria,
    CreateTaskField::Notes,
    CreateTaskField::Files,
];

#[derive(Debug, Clone)]
pub struct CreateTaskForm {
    pub title: Editor,
    pub epic: Editor,
    pub priority: Priority,
    pub assignee: Option<Assignee>,
    pub depends_on: Vec<TaskId>,
    pub dep_input: Editor,
    pub notes: Editor,
    pub acceptance_criteria: Vec<String>,
    pub ac_input: Editor,
    pub files: Vec<String>,
    pub files_input: Editor,
    pub active_field: CreateTaskField,
}

impl CreateTaskForm {
    pub fn new(default_epic: String) -> Self {
        let mut epic = Editor::new();
        epic.set_text(&default_epic);
        Self {
            title: Editor::new(),
            epic,
            priority: Priority::default(),
            assignee: None,
            depends_on: vec![],
            dep_input: Editor::new(),
            notes: Editor::new(),
            acceptance_criteria: vec![],
            ac_input: Editor::new(),
            files: vec![],
            files_input: Editor::new(),
            active_field: CreateTaskField::Title,
        }
    }

    pub fn next_field(&mut self) {
        let idx = FIELD_ORDER
            .iter()
            .position(|f| *f == self.active_field)
            .unwrap_or(0);
        self.active_field = FIELD_ORDER[(idx + 1) % FIELD_ORDER.len()];
    }

    pub fn prev_field(&mut self) {
        let idx = FIELD_ORDER
            .iter()
            .position(|f| *f == self.active_field)
            .unwrap_or(0);
        self.active_field = FIELD_ORDER[(idx + FIELD_ORDER.len() - 1) % FIELD_ORDER.len()];
    }

    pub fn cycle_priority_next(&mut self) {
        self.priority = match self.priority {
            Priority::Critical => Priority::High,
            Priority::High => Priority::Normal,
            Priority::Normal => Priority::Low,
            Priority::Low => Priority::Critical,
        };
    }

    pub fn cycle_priority_prev(&mut self) {
        self.priority = match self.priority {
            Priority::Critical => Priority::Low,
            Priority::High => Priority::Critical,
            Priority::Normal => Priority::High,
            Priority::Low => Priority::Normal,
        };
    }

    pub fn cycle_assignee_next(&mut self) {
        self.assignee = match &self.assignee {
            None => Some(Assignee::Claude),
            Some(Assignee::Claude) => Some(Assignee::Opencode),
            Some(Assignee::Opencode) => Some(Assignee::Human),
            Some(Assignee::Human) => None,
        };
    }

    pub fn cycle_assignee_prev(&mut self) {
        self.assignee = match &self.assignee {
            None => Some(Assignee::Human),
            Some(Assignee::Claude) => None,
            Some(Assignee::Opencode) => Some(Assignee::Claude),
            Some(Assignee::Human) => Some(Assignee::Opencode),
        };
    }

    pub fn try_commit_dep(&mut self) -> bool {
        let trimmed = self.dep_input.text_single_line();
        let trimmed = trimmed.trim().to_string();
        if trimmed.is_empty() {
            return false;
        }
        self.depends_on.push(TaskId(trimmed));
        self.dep_input.clear();
        true
    }

    pub fn try_commit_ac(&mut self) -> bool {
        let text = self.ac_input.text_single_line();
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return false;
        }
        self.acceptance_criteria.push(trimmed);
        self.ac_input.clear();
        true
    }

    pub fn try_commit_file(&mut self) -> bool {
        let text = self.files_input.text_single_line();
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return false;
        }
        self.files.push(trimmed);
        self.files_input.clear();
        true
    }

    pub fn assignee_label(&self) -> &str {
        match &self.assignee {
            None => "(none)",
            Some(Assignee::Claude) => "claude",
            Some(Assignee::Opencode) => "opencode",
            Some(Assignee::Human) => "human",
        }
    }
}
