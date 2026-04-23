use crate::cli::ImplArgs;
use crate::error::CliError;
use crate::output;
use std::io::{self, BufRead, Write};

use tc_core::config::{ExecutionMode, ExecutorKind, SandboxPolicy, TcConfig};
use tc_core::context::{ContextRenderer, build_resolved_deps};
use tc_core::dag::TaskDag;
use tc_core::status::{StatusId, StatusMachine};
use tc_core::task::Task;
use tc_executor::sandbox::sandbox_from_core;
use tc_executor::traits::{ExecutionRequest, Executor, SandboxConfig};
use tc_packer::{PackOptions, PackStyle};

pub fn run(args: ImplArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let mut tasks = store.load_tasks()?;
    let config = store.load_config()?;
    let sm = StatusMachine::new(config.statuses.clone());

    // Find task
    let task_idx = tasks
        .iter()
        .position(|t| t.id.0 == args.id)
        .ok_or_else(|| tc_core::error::CoreError::TaskNotFound(args.id.clone()))?;

    let task = &tasks[task_idx];

    // Validate: not terminal
    if sm.is_terminal(&task.status) {
        return Err(tc_core::error::CoreError::AlreadyTerminal {
            task: task.id.0.clone(),
            status: task.status.0.clone(),
        }
        .into());
    }

    // Validate: deps resolved
    let dag = TaskDag::from_tasks(&tasks)?;
    let unresolved: Vec<String> = dag
        .dependencies(&task.id)
        .iter()
        .filter(|dep_id| {
            tasks
                .iter()
                .find(|t| t.id == **dep_id)
                .is_some_and(|t| !sm.is_terminal(&t.status))
        })
        .map(|id| id.to_string())
        .collect();

    if !unresolved.is_empty() {
        return Err(tc_core::error::CoreError::unresolved_deps(&task.id.0, &unresolved).into());
    }

    // Build resolved deps description
    let resolved_deps = build_resolved_deps(&tasks, &dag, &task.id, &sm);

    // Pack files if task has file hints
    let packed_files = if args.pack || (!task.files.is_empty() && !args.dry_run) {
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

    // Render context
    let renderer = ContextRenderer::new(&config.context_template)?;
    let context = renderer.render(task, &resolved_deps, packed_files.as_deref())?;

    // --dry-run: print context and exit
    if args.dry_run {
        println!("{context}");
        return Ok(());
    }

    // Set status -> in_progress
    tasks[task_idx].status = StatusId::in_progress();
    store.save_tasks(&tasks)?;

    // Determine execution mode: CLI flags win over config; otherwise fall
    // back to `config.executor.mode`, which the user sets in TUI settings.
    let mode = if args.yolo {
        ExecutionMode::Yolo
    } else if args.accept {
        ExecutionMode::Accept
    } else {
        config.executor.mode
    };

    let sandbox = if args.no_sandbox {
        SandboxConfig {
            enabled: SandboxPolicy::Never,
            extra_allow: vec![],
            block_network: false,
        }
    } else {
        sandbox_from_core(&config.executor.sandbox)
    };

    let request = ExecutionRequest {
        context: context.clone(),
        mode,
        working_dir: store.root().clone(),
        sandbox,
        mcp_servers: vec![],
    };

    let context_path = store.context_path();
    if !matches!(mode, ExecutionMode::Yolo) {
        write_context_file(&context_path, &context)?;
    }

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| CliError::User(format!("failed to create runtime: {e}")))?;

    let result = rt.block_on(run_executor(&args, &config, &request))?;

    let _ = std::fs::remove_file(&context_path);

    let mut tasks = store.load_tasks()?;
    let task_idx = tasks
        .iter()
        .position(|t| t.id.0 == args.id)
        .ok_or_else(|| tc_core::error::CoreError::TaskNotFound(args.id.clone()))?;

    if !args.no_verify && !config.verification.commands.is_empty() && result.exit_code == 0 {
        let verify_result = rt.block_on(run_verification(&config, store.root()))?;

        if verify_result {
            tasks[task_idx].status = StatusId(config.verification.on_pass.clone());
            store.save_tasks(&tasks)?;
            output::print_success(&format!(
                "{} verification passed -> {}",
                args.id, config.verification.on_pass
            ));
        } else {
            tasks[task_idx].status = StatusId(config.verification.on_fail.clone());
            store.save_tasks(&tasks)?;
            output::print_error(&format!(
                "{} verification failed -> {}",
                args.id, config.verification.on_fail
            ));
        }
    } else if result.exit_code == 0 {
        // No verification -- prompt user
        prompt_completion(&store, &mut tasks, task_idx, &args.id)?;
    } else {
        tasks[task_idx].status = StatusId::blocked();
        append_note(
            &mut tasks[task_idx].notes,
            &format!("BLOCKED: agent exited with code {}", result.exit_code),
        );
        store.save_tasks(&tasks)?;
        output::print_error(&format!(
            "{} agent exited with code {}",
            args.id, result.exit_code
        ));
    }

    Ok(())
}

fn combine_exclude(config: &TcConfig, task: &Task) -> Vec<String> {
    let mut patterns = config.packer.ignore_patterns.clone();
    patterns.extend(task.pack_exclude.clone());
    patterns
}

pub(crate) fn append_note(notes: &mut String, note: &str) {
    if !notes.is_empty() {
        notes.push('\n');
    }
    notes.push_str(note);
}

fn write_context_file(path: &std::path::Path, content: &str) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::User(format!("failed to create dir: {e}")))?;
    }
    std::fs::write(path, content)
        .map_err(|e| CliError::User(format!("failed to write context: {e}")))?;
    Ok(())
}

async fn run_executor(
    args: &ImplArgs,
    config: &TcConfig,
    request: &ExecutionRequest,
) -> Result<tc_executor::traits::ExecutionResult, CliError> {
    let kind = if args.opencode {
        ExecutorKind::Opencode
    } else {
        config.executor.default
    };
    let executor = tc_executor::any::executor_by_kind(kind, config)?;
    executor
        .execute(request, None)
        .await
        .map_err(CliError::from)
}

async fn run_verification(
    config: &TcConfig,
    working_dir: &std::path::Path,
) -> Result<bool, CliError> {
    let result = tc_executor::verify::run_verification(&config.verification.commands, working_dir)
        .await
        .map_err(CliError::from)?;

    if !result.passed {
        let diag = tc_executor::verify::format_failure_diagnostics(&result);
        eprintln!("{diag}");
    }

    Ok(result.passed)
}

fn prompt_completion(
    store: &tc_storage::Store,
    tasks: &mut [Task],
    task_idx: usize,
    task_id: &str,
) -> Result<(), CliError> {
    eprint!("Mark {task_id} as done? [y/n/review] ");
    io::stderr().flush().ok();

    let stdin = io::stdin();
    let line = stdin
        .lock()
        .lines()
        .next()
        .unwrap_or(Ok(String::new()))
        .unwrap_or_default();

    let new_status = match line.trim().to_lowercase().as_str() {
        "y" | "yes" => "done",
        "r" | "review" => "review",
        _ => {
            output::print_warning(&format!("{task_id} left as in_progress"));
            return Ok(());
        }
    };

    tasks[task_idx].status = StatusId(new_status.into());
    store.save_tasks(tasks)?;
    output::print_success(&format!("{task_id} -> {new_status}"));
    Ok(())
}
