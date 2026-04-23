use std::process::Command;

use tc_core::status::{StatusId, StatusMachine};
use tc_core::task::TaskId;
use tc_spawn::merge::{MergeResult, merge_worktree};
use tc_spawn::worktree::WorktreeManager;

use crate::cli::{MergeArgs, ReviewArgs};
use crate::error::CliError;
use crate::output;

pub fn run(args: ReviewArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let config = store.load_config()?;
    let task_id = TaskId(args.id.clone());

    let worktree_mgr = WorktreeManager::new(store.root().clone(), config.spawn.clone());
    let wt_info = worktree_mgr
        .find(&task_id)?
        .ok_or_else(|| CliError::user(format!("no worktree found for {}", args.id)))?;

    if let Some(feedback) = args.reject {
        // Reject: add feedback to notes, set status -> todo
        let mut tasks = store.load_tasks()?;
        let task = tasks.iter_mut().find(|t| t.id == task_id).ok_or_else(|| {
            CliError::from(tc_core::error::CoreError::TaskNotFound(args.id.clone()))
        })?;

        if !task.notes.is_empty() {
            task.notes.push('\n');
        }
        task.notes.push_str(&format!("REJECTED: {feedback}"));
        task.status = StatusId("todo".into());
        store.save_tasks(&tasks)?;

        output::print_warning(&format!("{} rejected -- feedback saved", args.id));
        return Ok(());
    }

    // Show diff in $PAGER
    let branch = &wt_info.branch;
    let base = &config.spawn.base_branch;
    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".into());

    let diff_output = Command::new("git")
        .args(["diff", &format!("{base}...{branch}")])
        .current_dir(store.root())
        .output()
        .map_err(|e| CliError::user(format!("git diff failed: {e}")))?;

    if diff_output.stdout.is_empty() {
        println!("No changes in worktree for {}.", args.id);
        return Ok(());
    }

    // Pipe diff through pager
    let mut pager_cmd = Command::new(&pager)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| CliError::user(format!("failed to open pager '{pager}': {e}")))?;

    if let Some(ref mut stdin) = pager_cmd.stdin {
        use std::io::Write;
        let _ = stdin.write_all(&diff_output.stdout);
    }

    let _ = pager_cmd.wait();

    Ok(())
}

pub fn run_merge(args: MergeArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let config = store.load_config()?;
    let worktree_mgr = WorktreeManager::new(store.root().clone(), config.spawn.clone());

    if args.all {
        return merge_all(&store, &worktree_mgr, &config);
    }

    let id = args
        .id
        .ok_or_else(|| CliError::user("task ID required (or use --all)"))?;

    let task_id = TaskId(id.clone());

    let tasks = store.load_tasks()?;
    let task_title = tasks
        .iter()
        .find(|t| t.id == task_id)
        .map(|t| t.title.clone())
        .ok_or_else(|| CliError::from(tc_core::error::CoreError::TaskNotFound(id.clone())))?;

    match merge_worktree(&worktree_mgr, &task_id, &task_title)? {
        MergeResult::Success => {
            // Update task status -> done
            let mut tasks = store.load_tasks()?;
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = StatusId("done".into());
            }
            store.save_tasks(&tasks)?;

            output::print_success(&format!("{id} merged successfully"));
        }
        MergeResult::Conflict { details } => {
            output::print_error(&format!("{id} merge conflict -- worktree preserved"));
            eprintln!("{details}");
        }
    }

    Ok(())
}

fn merge_all(
    store: &tc_storage::Store,
    worktree_mgr: &WorktreeManager,
    config: &tc_core::config::TcConfig,
) -> Result<(), CliError> {
    let tasks = store.load_tasks()?;
    let sm = StatusMachine::new(config.statuses.clone());

    // Find tasks that are done or in review status and have worktrees
    let mergeable: Vec<(TaskId, String)> = tasks
        .iter()
        .filter(|t| {
            let status = &t.status.0;
            status == "done" || (status == "review" && !sm.is_terminal(&t.status))
        })
        .map(|t| (t.id.clone(), t.title.clone()))
        .collect();

    if mergeable.is_empty() {
        output::print_success("no tasks ready to merge");
        return Ok(());
    }

    let mut merged = 0;
    let mut conflicts = 0;

    // Serial loop -- parallel rebases against a moving base would race.
    'merge: for (task_id, task_title) in &mergeable {
        // Check if worktree exists
        let wt = worktree_mgr.find(task_id)?;
        if wt.is_none() {
            continue 'merge;
        }

        match merge_worktree(worktree_mgr, task_id, task_title)? {
            MergeResult::Success => {
                let mut tasks = store.load_tasks()?;
                if let Some(task) = tasks.iter_mut().find(|t| t.id == *task_id) {
                    task.status = StatusId("done".into());
                }
                store.save_tasks(&tasks)?;

                output::print_success(&format!("{} merged", task_id));
                merged += 1;
            }
            MergeResult::Conflict { details } => {
                output::print_error(&format!("{} merge conflict", task_id));
                eprintln!("{details}");
                conflicts += 1;
            }
        }
    }

    output::print_success(&format!(
        "merge complete: {merged} merged, {conflicts} conflicts"
    ));
    Ok(())
}
