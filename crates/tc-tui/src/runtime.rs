use std::io;
use std::process::Command;
use std::time::{Duration, Instant};

use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tc_core::context::{ContextRenderer, build_resolved_deps};
use tc_core::dag::TaskDag;
use tc_core::status::{StatusId, StatusMachine};
use tc_core::task::TaskId;
use tc_executor::sandbox::sandbox_from_core;
use tc_executor::traits::{ExecutionMode, ExecutionRequest};
use tc_storage::Store;

use crate::app::{App, TuiAction};
use crate::error::{TuiError, TuiResult};
use crate::event::{Message, poll};
use crate::ui;

const TICK: Duration = Duration::from_millis(250);

/// Top-level entry: discover store, install panic hook, run event loop.
pub fn run() -> TuiResult<()> {
    let store = Store::discover()?;
    let mut app = App::new(store)?;

    install_panic_hook();
    setup_terminal()?;

    let result = run_loop(&mut app);

    let _ = teardown_terminal();
    result
}

fn run_loop(app: &mut App) -> TuiResult<()> {
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).map_err(TuiError::from)?;
    let mut last_tick = Instant::now();

    'main_loop: loop {
        terminal
            .draw(|f| ui::render(app, f))
            .map_err(TuiError::from)?;

        let tick_remaining = TICK.saturating_sub(last_tick.elapsed());
        let mut timeout = tick_remaining;
        if let Some(wake) = app.chord_wake_in()
            && wake < timeout
        {
            timeout = wake;
        }
        if let Some(wake) = app.animation_wake_in()
            && wake < timeout
        {
            timeout = wake;
        }
        if let Some(msg) = poll(timeout)? {
            app.update(msg)?;
        }

        if last_tick.elapsed() >= TICK {
            app.update(Message::Tick)?;
            last_tick = Instant::now();
        }

        // Handle actions that require suspending the TUI
        match app.take_action() {
            TuiAction::None => {}
            TuiAction::SuspendForImpl(task_id) => {
                teardown_terminal()?;
                let result = run_impl_suspended(app, &task_id);
                setup_terminal()?;
                terminal =
                    Terminal::new(CrosstermBackend::new(io::stdout())).map_err(TuiError::from)?;
                app.reload_tasks()?;
                app.refresh_workers();
                match result {
                    Ok(msg) => app.toast(&msg),
                    Err(e) => app.toast(&format!("start failed: {e}")),
                }
            }
            TuiAction::SuspendForReview(task_id) => {
                teardown_terminal()?;
                let result = run_review_suspended(app, &task_id);
                setup_terminal()?;
                terminal =
                    Terminal::new(CrosstermBackend::new(io::stdout())).map_err(TuiError::from)?;
                match result {
                    Ok(()) => app.toast(&format!("review of {} complete", task_id)),
                    Err(e) => app.toast(&format!("review failed: {e}")),
                }
            }
        }

        if !app.running {
            break 'main_loop;
        }
    }
    Ok(())
}

/// Run interactive `claude` for a task (called while TUI is suspended).
fn run_impl_suspended(app: &App, task_id: &TaskId) -> Result<String, TuiError> {
    let tasks = app.store.load_tasks()?;
    let task = tasks
        .iter()
        .find(|t| t.id == *task_id)
        .ok_or_else(|| TuiError::Render(format!("task {} not found", task_id)))?;

    let sm = StatusMachine::new(app.config.statuses.clone());
    let dag = TaskDag::from_tasks(&tasks)?;
    let resolved_deps = build_resolved_deps(&tasks, &dag, task_id, &sm);
    let renderer = ContextRenderer::new(&app.config.context_template)?;
    let context = renderer.render(task, &resolved_deps, None)?;
    let sandbox = sandbox_from_core(&app.config.executor.sandbox);

    let request = ExecutionRequest {
        context,
        mode: ExecutionMode::Interactive,
        working_dir: app.store.root().clone(),
        sandbox,
        mcp_servers: vec![],
    };

    // Write context file for the agent
    let context_path = app.store.context_path();
    if let Some(parent) = context_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&context_path, &request.context);

    // Run claude interactively with inherited stdio
    let status = Command::new("claude")
        .arg(&request.context)
        .current_dir(&request.working_dir)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| TuiError::Render(format!("failed to run claude: {e}")))?;

    let _ = std::fs::remove_file(&context_path);

    // Update task status
    let mut tasks = app.store.load_tasks()?;
    if let Some(target) = tasks.iter_mut().find(|t| t.id == *task_id) {
        if status.success() {
            target.status = StatusId::review();
        } else {
            target.status = StatusId::blocked();
            if !target.notes.is_empty() {
                target.notes.push('\n');
            }
            target.notes.push_str(&format!(
                "BLOCKED: agent exited with code {}",
                status.code().unwrap_or(-1)
            ));
        }
    }
    app.store.save_tasks(&tasks)?;

    let exit_code = status.code().unwrap_or(-1);
    if status.success() {
        Ok(format!("{task_id} impl complete -> review"))
    } else {
        Ok(format!("{task_id} agent exited with code {exit_code}"))
    }
}

/// Show diff in $PAGER for a task (called while TUI is suspended).
fn run_review_suspended(app: &App, task_id: &TaskId) -> Result<(), TuiError> {
    let worktree_mgr = tc_spawn::worktree::WorktreeManager::new(
        app.store.root().clone(),
        app.config.spawn.clone(),
    );
    let wt_info = worktree_mgr
        .find(task_id)?
        .ok_or_else(|| TuiError::Render(format!("no worktree for {task_id}")))?;

    let branch = &wt_info.branch;
    let base = &app.config.spawn.base_branch;
    let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".into());

    let diff_output = Command::new("git")
        .args(["diff", &format!("{base}...{branch}")])
        .current_dir(app.store.root())
        .output()
        .map_err(|e| TuiError::Render(format!("git diff failed: {e}")))?;

    if diff_output.stdout.is_empty() {
        println!("No changes in worktree for {task_id}.");
        // Brief pause so user can read the message
        std::thread::sleep(Duration::from_secs(1));
        return Ok(());
    }

    let mut pager_cmd = Command::new(&pager)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| TuiError::Render(format!("failed to open pager '{pager}': {e}")))?;

    if let Some(ref mut stdin) = pager_cmd.stdin {
        use std::io::Write;
        let _ = stdin.write_all(&diff_output.stdout);
    }

    let _ = pager_cmd.wait();
    Ok(())
}

fn setup_terminal() -> TuiResult<()> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;
    if supports_keyboard_enhancement().unwrap_or(false) {
        execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    Ok(())
}

fn teardown_terminal() -> TuiResult<()> {
    if supports_keyboard_enhancement().unwrap_or(false) {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original(info);
    }));
}
