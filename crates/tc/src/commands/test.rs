use std::path::Path;
use std::process::Command;

use chrono::Utc;
use tc_core::config::{TcConfig, TesterConfig};
use tc_core::status::StatusId;
use tc_core::task::{Task, TaskId};
use tc_executor::tester::{TesterExecutor, TesterVerdict};
use tc_executor::traits::{
    ExecutionMode, ExecutionRequest, Executor, McpServer, SandboxConfig, SandboxPolicy,
};
use tc_spawn::worktree::WorktreeManager;
use tc_storage::Store;

use crate::cli::TestArgs;
use crate::error::CliError;
use crate::output;

const DEFAULT_TESTER_PROMPT: &str = "\
You are a testing agent. Your job is to verify that the implementation \
of a task is correct and complete.

Review the code changes, run the project's test suite, and verify the \
implementation meets the acceptance criteria. If browser testing tools \
are available, use them to verify UI changes.

Report your findings clearly:
- What tests pass/fail
- Whether acceptance criteria are met
- Any issues or bugs found
- Whether the code follows project conventions

When you finish testing, you MUST write a verdict file at \
`.tc/.tester_verdict.json` in the project root before exiting. Format:
  {\"verdict\": \"pass\"}  -- when all acceptance criteria pass and tests are green.
  {\"verdict\": \"fail\", \"reason\": \"<concise explanation of what failed and how to fix it>\"}  -- otherwise.
Write this file exactly once. Do not commit it.";

const DEFAULT_BROWSER_MCP_COMMAND: &str = "npx @anthropic-ai/claude-code-mcp-server-playwright";

pub async fn run(args: TestArgs) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let config = store.load_config()?;
    let mut tasks = store.load_tasks()?;
    let task_id = TaskId(args.id.clone());

    let task = tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| tc_core::error::CoreError::TaskNotFound(args.id.clone()))?;

    // Determine working directory: worktree if it exists, else project root
    let worktree_mgr = WorktreeManager::new(store.root().clone(), config.spawn.clone());
    let (working_dir, diff) = match worktree_mgr.find(&task_id)? {
        Some(wt_info) => {
            let diff = get_worktree_diff(store.root(), &config.spawn.base_branch, &wt_info.branch)?;
            (wt_info.path, diff)
        }
        None => {
            output::print_warning("no worktree found -- testing in project root");
            (store.root().clone(), String::new())
        }
    };

    let context = build_test_context(task, &diff);

    let mcp_servers = resolve_mcp_servers(&config.tester, &args);

    let system_prompt = config
        .tester
        .as_ref()
        .map(|c| &c.system_prompt)
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| DEFAULT_TESTER_PROMPT.to_string());

    // Write context file so the agent can reference it
    let context_path = store.context_path();
    write_context_file(&context_path, &context)?;

    let verdict_path = store.verdict_path();
    // Clear any stale verdict from a previous run
    let _ = std::fs::remove_file(&verdict_path);

    let request = ExecutionRequest {
        context,
        mode: ExecutionMode::Interactive,
        working_dir,
        sandbox: SandboxConfig {
            enabled: SandboxPolicy::Never,
            extra_allow: vec![],
            block_network: false,
        },
        mcp_servers,
    };

    let executor = TesterExecutor { system_prompt };

    let result = executor.execute(&request, None).await?;

    let _ = std::fs::remove_file(&context_path);

    let verdict = tc_executor::tester::read_verdict(&verdict_path)?;
    let _ = std::fs::remove_file(&verdict_path);

    match verdict {
        Some(TesterVerdict::Pass { reason }) => {
            apply_pass(&store, &config, &mut tasks, &task_id, reason.as_deref())?;
            output::print_success(&format!("{} passed testing", args.id));
        }
        Some(TesterVerdict::Fail { reason }) => {
            let fix_id = apply_fail(&store, &config, &mut tasks, &task_id, &reason)?;
            output::print_warning(&format!("{} failed testing: {reason}", args.id));
            output::print_success(&format!("Created fix task: {}", fix_id.0));
        }
        None => {
            if result.exit_code == 0 {
                output::print_warning(&format!(
                    "{} tester produced no verdict -- no status change",
                    args.id
                ));
            } else {
                output::print_error(&format!(
                    "{} tester exited with code {} and produced no verdict",
                    args.id, result.exit_code
                ));
            }
        }
    }

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────

fn get_worktree_diff(
    root: &Path,
    base_branch: &str,
    task_branch: &str,
) -> Result<String, CliError> {
    let output = Command::new("git")
        .args(["diff", &format!("{base_branch}...{task_branch}")])
        .current_dir(root)
        .output()
        .map_err(|e| CliError::user(format!("git diff failed: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn build_test_context(task: &Task, diff: &str) -> String {
    let mut ctx = format!("# Test Task {}: {}\n\n", task.id, task.title);
    ctx.push_str(&format!("**Epic:** {}\n\n", task.epic));

    if !task.acceptance_criteria.is_empty() {
        ctx.push_str("## Acceptance Criteria\n");
        for ac in &task.acceptance_criteria {
            ctx.push_str(&format!("- {ac}\n"));
        }
        ctx.push('\n');
    }

    if !task.notes.is_empty() {
        ctx.push_str("## Notes\n");
        ctx.push_str(&task.notes);
        ctx.push_str("\n\n");
    }

    if !diff.is_empty() {
        ctx.push_str("## Changes to Review\n```diff\n");
        ctx.push_str(diff);
        ctx.push_str("```\n\n");
    }

    ctx.push_str(
        "## Instructions\n\
         Verify the implementation is correct and complete. \
         Run the project's test suite, check for edge cases, \
         and ensure all acceptance criteria are met.\n",
    );

    ctx
}

fn resolve_mcp_servers(tester_config: &Option<TesterConfig>, args: &TestArgs) -> Vec<McpServer> {
    if args.no_mcp {
        return vec![];
    }

    let mut servers: Vec<McpServer> = tester_config
        .as_ref()
        .map(|c| {
            c.mcp
                .iter()
                .map(|m| McpServer {
                    name: m.name.clone(),
                    command: m.command.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    if args.browser && !servers.iter().any(|s| s.name == "browser") {
        servers.push(McpServer {
            name: "browser".to_string(),
            command: DEFAULT_BROWSER_MCP_COMMAND.to_string(),
        });
    }

    servers
}

fn write_context_file(path: &Path, content: &str) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CliError::user(format!("failed to create dir: {e}")))?;
    }
    std::fs::write(path, content)
        .map_err(|e| CliError::user(format!("failed to write context: {e}")))?;
    Ok(())
}

fn apply_pass(
    store: &Store,
    config: &TcConfig,
    tasks: &mut [Task],
    id: &TaskId,
    reason: Option<&str>,
) -> Result<(), CliError> {
    let idx = tasks
        .iter()
        .position(|t| &t.id == id)
        .ok_or_else(|| tc_core::error::CoreError::TaskNotFound(id.0.clone()))?;

    tasks[idx].status = StatusId(config.verification.on_pass.clone());
    if let Some(r) = reason {
        append_note(&mut tasks[idx].notes, &format!("PASS: {r}"));
    }
    store.save_tasks(tasks)?;
    Ok(())
}

fn apply_fail(
    store: &Store,
    config: &TcConfig,
    tasks: &mut Vec<Task>,
    id: &TaskId,
    reason: &str,
) -> Result<TaskId, CliError> {
    let idx = tasks
        .iter()
        .position(|t| &t.id == id)
        .ok_or_else(|| tc_core::error::CoreError::TaskNotFound(id.0.clone()))?;

    tasks[idx].status = StatusId(config.verification.on_fail.clone());
    append_note(&mut tasks[idx].notes, &format!("FAILED: {reason}"));

    let fix_id = store.next_task_id(tasks);
    let fix = build_fix_task(fix_id.clone(), &tasks[idx], reason);
    tasks.push(fix);

    store.save_tasks(tasks)?;
    Ok(fix_id)
}

fn build_fix_task(fix_id: TaskId, original: &Task, reason: &str) -> Task {
    Task {
        id: fix_id,
        title: format!("Fix: {}", original.title),
        epic: original.epic.clone(),
        status: StatusId("todo".to_string()),
        priority: original.priority,
        depends_on: vec![original.id.clone()],
        files: vec![],
        pack_exclude: vec![],
        notes: reason.to_string(),
        acceptance_criteria: vec![],
        assignee: None,
        created_at: Utc::now(),
    }
}

fn append_note(notes: &mut String, line: &str) {
    if !notes.is_empty() {
        notes.push('\n');
    }
    notes.push_str(line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tc_core::config::McpServerConfig;
    use tc_core::status::StatusId;

    fn make_task() -> Task {
        Task {
            id: TaskId("T-001".into()),
            title: "Implement login".into(),
            epic: "auth".into(),
            status: StatusId("in_progress".into()),
            priority: tc_core::task::Priority::default(),
            depends_on: vec![],
            files: vec![],
            pack_exclude: vec![],
            notes: String::new(),
            acceptance_criteria: vec![],
            assignee: None,
            created_at: Utc::now(),
        }
    }

    // ── build_test_context ─────────────────────────────────────────

    #[test]
    fn context_includes_task_header() {
        let task = make_task();
        let ctx = build_test_context(&task, "");
        assert!(ctx.contains("# Test Task T-001: Implement login"));
        assert!(ctx.contains("**Epic:** auth"));
    }

    #[test]
    fn context_includes_diff_when_present() {
        let task = make_task();
        let diff = "+fn new_function() {}";
        let ctx = build_test_context(&task, diff);
        assert!(ctx.contains("## Changes to Review"));
        assert!(ctx.contains(diff));
    }

    #[test]
    fn context_omits_diff_section_when_empty() {
        let task = make_task();
        let ctx = build_test_context(&task, "");
        assert!(!ctx.contains("## Changes to Review"));
    }

    #[test]
    fn context_includes_acceptance_criteria() {
        let mut task = make_task();
        task.acceptance_criteria = vec!["Login works".into(), "Tests pass".into()];
        let ctx = build_test_context(&task, "");
        assert!(ctx.contains("## Acceptance Criteria"));
        assert!(ctx.contains("- Login works"));
        assert!(ctx.contains("- Tests pass"));
    }

    #[test]
    fn context_omits_acceptance_criteria_when_empty() {
        let task = make_task();
        let ctx = build_test_context(&task, "");
        assert!(!ctx.contains("## Acceptance Criteria"));
    }

    #[test]
    fn context_includes_notes() {
        let mut task = make_task();
        task.notes = "Use OAuth2 flow".into();
        let ctx = build_test_context(&task, "");
        assert!(ctx.contains("## Notes"));
        assert!(ctx.contains("Use OAuth2 flow"));
    }

    #[test]
    fn context_omits_notes_when_empty() {
        let task = make_task();
        let ctx = build_test_context(&task, "");
        assert!(!ctx.contains("## Notes"));
    }

    #[test]
    fn context_always_includes_instructions() {
        let task = make_task();
        let ctx = build_test_context(&task, "");
        assert!(ctx.contains("## Instructions"));
        assert!(ctx.contains("Verify the implementation"));
    }

    // ── resolve_mcp_servers ────────────────────────────────────────

    #[test]
    fn mcp_no_mcp_flag_returns_empty() {
        let args = TestArgs {
            id: "T-001".into(),
            browser: true,
            no_mcp: true,
        };
        let servers = resolve_mcp_servers(&None, &args);
        assert!(servers.is_empty());
    }

    #[test]
    fn mcp_browser_flag_adds_browser() {
        let args = TestArgs {
            id: "T-001".into(),
            browser: true,
            no_mcp: false,
        };
        let servers = resolve_mcp_servers(&None, &args);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "browser");
    }

    #[test]
    fn mcp_browser_flag_no_duplicate() {
        let args = TestArgs {
            id: "T-001".into(),
            browser: true,
            no_mcp: false,
        };
        let config = Some(TesterConfig {
            executor: tc_core::config::ExecutorKind::Claude,
            mcp: vec![McpServerConfig {
                name: "browser".into(),
                command: "custom-browser-cmd".into(),
            }],
            system_prompt: String::new(),
        });
        let servers = resolve_mcp_servers(&config, &args);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].command, "custom-browser-cmd");
    }

    #[test]
    fn mcp_from_config() {
        let args = TestArgs {
            id: "T-001".into(),
            browser: false,
            no_mcp: false,
        };
        let config = Some(TesterConfig {
            executor: tc_core::config::ExecutorKind::Claude,
            mcp: vec![
                McpServerConfig {
                    name: "db".into(),
                    command: "db-mcp-server".into(),
                },
                McpServerConfig {
                    name: "api".into(),
                    command: "api-mcp-server".into(),
                },
            ],
            system_prompt: String::new(),
        });
        let servers = resolve_mcp_servers(&config, &args);
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "db");
        assert_eq!(servers[1].name, "api");
    }

    #[test]
    fn mcp_no_config_no_flags_returns_empty() {
        let args = TestArgs {
            id: "T-001".into(),
            browser: false,
            no_mcp: false,
        };
        let servers = resolve_mcp_servers(&None, &args);
        assert!(servers.is_empty());
    }

    // ── build_fix_task ─────────────────────────────────────────────

    #[test]
    fn build_fix_task_title_prefixed_with_fix() {
        let original = make_task();
        let fix = build_fix_task(TaskId("T-002".into()), &original, "login broken");
        assert_eq!(fix.title, "Fix: Implement login");
    }

    #[test]
    fn build_fix_task_copies_epic_and_depends_on_original() {
        let original = make_task();
        let fix = build_fix_task(TaskId("T-002".into()), &original, "login broken");
        assert_eq!(fix.epic, original.epic);
        assert_eq!(fix.depends_on, vec![original.id.clone()]);
        assert_eq!(fix.status.0, "todo");
        assert_eq!(fix.notes, "login broken");
        assert_eq!(fix.id.0, "T-002");
    }

    // ── apply_pass / apply_fail ────────────────────────────────────

    fn make_store_with_tasks(tasks: &[Task]) -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = tc_storage::init::init_project(dir.path()).expect("init");
        store.save_tasks(tasks).expect("save");
        (dir, store)
    }

    fn make_config() -> TcConfig {
        let store_dir = tempfile::tempdir().expect("tempdir");
        let store = tc_storage::init::init_project(store_dir.path()).expect("init");
        store.load_config().expect("load")
    }

    #[test]
    fn apply_pass_sets_on_pass_status_and_appends_reason() {
        let task = make_task();
        let (_dir, store) = make_store_with_tasks(std::slice::from_ref(&task));
        let config = make_config();
        let mut tasks = vec![task.clone()];

        apply_pass(&store, &config, &mut tasks, &task.id, Some("all green")).expect("pass");

        assert_eq!(tasks[0].status.0, config.verification.on_pass);
        assert!(tasks[0].notes.contains("PASS: all green"));
    }

    #[test]
    fn apply_pass_without_reason_leaves_notes_empty() {
        let task = make_task();
        let (_dir, store) = make_store_with_tasks(std::slice::from_ref(&task));
        let config = make_config();
        let mut tasks = vec![task.clone()];

        apply_pass(&store, &config, &mut tasks, &task.id, None).expect("pass");

        assert_eq!(tasks[0].status.0, config.verification.on_pass);
        assert!(tasks[0].notes.is_empty());
    }

    #[test]
    fn apply_fail_sets_on_fail_status_and_creates_fix_task() {
        let task = make_task();
        let (_dir, store) = make_store_with_tasks(std::slice::from_ref(&task));
        let config = make_config();
        let mut tasks = vec![task.clone()];

        let fix_id =
            apply_fail(&store, &config, &mut tasks, &task.id, "auth broken").expect("fail");

        assert_eq!(tasks[0].status.0, config.verification.on_fail);
        assert!(tasks[0].notes.contains("FAILED: auth broken"));
        assert_eq!(tasks.len(), 2);
        let fix = &tasks[1];
        assert_eq!(fix.id, fix_id);
        assert_eq!(fix.title, "Fix: Implement login");
        assert_eq!(fix.depends_on, vec![task.id.clone()]);
        assert_eq!(fix.status.0, "todo");
        assert_eq!(fix.notes, "auth broken");
    }

    #[test]
    fn apply_fail_appends_reason_when_notes_nonempty() {
        let mut task = make_task();
        task.notes = "existing note".into();
        let (_dir, store) = make_store_with_tasks(std::slice::from_ref(&task));
        let config = make_config();
        let mut tasks = vec![task.clone()];

        apply_fail(&store, &config, &mut tasks, &task.id, "bug").expect("fail");

        assert!(tasks[0].notes.starts_with("existing note"));
        assert!(tasks[0].notes.contains("FAILED: bug"));
        assert!(tasks[0].notes.contains('\n'));
    }
}
