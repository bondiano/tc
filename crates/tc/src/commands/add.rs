use chrono::Utc;
use tc_core::status::StatusId;
use tc_core::task::{Priority, Task};

use crate::cli::AddArgs;
use crate::error::CliError;
use crate::output;

pub fn run(args: AddArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;
    let id = store.next_task_id(&tasks);

    let task = Task {
        id: id.clone(),
        title: args.title.clone(),
        epic: args.epic,
        status: StatusId("todo".to_string()),
        depends_on: args
            .after
            .unwrap_or_default()
            .into_iter()
            .map(tc_core::task::TaskId)
            .collect(),
        priority: Priority::from(args.priority),
        files: args.files.unwrap_or_default(),
        pack_exclude: vec![],
        notes: String::new(),
        acceptance_criteria: args.acceptance_criteria.unwrap_or_default(),
        assignee: None,
        created_at: Utc::now(),
    };

    tasks.push(task);
    store.save_tasks(&tasks)?;

    output::print_success(&format!("Created {id}: {}", args.title));
    Ok(())
}
