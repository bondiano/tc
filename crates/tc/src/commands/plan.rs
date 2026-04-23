use crate::cli::PlanArgs;
use crate::commands::impl_::append_note;
use crate::error::CliError;
use crate::output;

use tc_core::config::TcConfig;
use tc_core::context::{ContextRenderer, build_resolved_deps};
use tc_core::dag::TaskDag;
use tc_core::status::StatusMachine;
use tc_core::task::Task;
use tc_packer::{PackOptions, PackStyle};

pub fn run(args: PlanArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let tasks = store.load_tasks()?;
    let config = store.load_config()?;
    let sm = StatusMachine::new(config.statuses.clone());

    let task = tasks
        .iter()
        .find(|t| t.id.0 == args.id)
        .ok_or_else(|| tc_core::error::CoreError::TaskNotFound(args.id.clone()))?;

    // Validate: not terminal
    if sm.is_terminal(&task.status) {
        return Err(tc_core::error::CoreError::AlreadyTerminal {
            task: task.id.0.clone(),
            status: task.status.0.clone(),
        }
        .into());
    }

    // Build resolved deps description
    let dag = TaskDag::from_tasks(&tasks)?;
    let resolved_deps = build_resolved_deps(&tasks, &dag, &task.id, &sm);

    // Pack files if task has file hints
    let packed_files = if args.pack || !task.files.is_empty() {
        let style = PackStyle::from(config.packer.style.as_str());
        let options = PackOptions {
            root: store.root().clone(),
            include_paths: task.files.clone(),
            exclude_patterns: combine_exclude(&config, task),
            token_budget: config.packer.token_budget,
            style,
        };
        match tc_packer::pack(&options) {
            Ok(result) => {
                for w in &result.warnings {
                    output::print_warning(w);
                }
                Some(result.content)
            }
            Err(e) => {
                output::print_warning(&format!("pack failed: {e}"));
                None
            }
        }
    } else {
        None
    };

    // Render plan prompt using plan_template
    let renderer = ContextRenderer::new(&config.plan_template)?;
    let prompt = renderer.render(task, &resolved_deps, packed_files.as_deref())?;

    // --dry-run: print prompt and exit
    if args.dry_run {
        println!("{prompt}");
        return Ok(());
    }

    let program = if args.opencode { "opencode" } else { "claude" };
    if which::which(program).is_err() {
        return Err(tc_executor::error::ExecutorError::not_found(program).into());
    }

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| CliError::user(format!("failed to create runtime: {e}")))?;

    let plan_output = rt.block_on(run_plan_agent(program, &prompt, store.root()))?;

    println!("{plan_output}");

    if args.save {
        let mut tasks = store.load_tasks()?;
        let task_idx = tasks
            .iter()
            .position(|t| t.id.0 == args.id)
            .ok_or_else(|| tc_core::error::CoreError::TaskNotFound(args.id.clone()))?;

        append_note(
            &mut tasks[task_idx].notes,
            &format!("## Plan\n{plan_output}"),
        );
        store.save_tasks(&tasks)?;
        output::print_success(&format!("{} plan saved to notes", args.id));
    }

    Ok(())
}

fn combine_exclude(config: &TcConfig, task: &Task) -> Vec<String> {
    let mut patterns = config.packer.ignore_patterns.clone();
    patterns.extend(task.pack_exclude.clone());
    patterns
}

async fn run_plan_agent(
    program: &str,
    prompt: &str,
    working_dir: &std::path::Path,
) -> Result<String, CliError> {
    let mut args: Vec<String> = Vec::new();

    if program == "claude" {
        args.push("--print".to_string());
        args.push(prompt.to_string());
    } else {
        // opencode
        args.push("--yes".to_string());
        args.push("--prompt".to_string());
        args.push(prompt.to_string());
    }

    let output = tokio::process::Command::new(program)
        .args(&args)
        .current_dir(working_dir)
        .output()
        .await
        .map_err(|e| tc_executor::error::ExecutorError::spawn_failed(program, e))?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        return Err(tc_executor::error::ExecutorError::non_zero_exit(program, code).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}
