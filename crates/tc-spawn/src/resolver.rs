use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::Duration;

use minijinja::Environment;
use tc_core::config::TcConfig;
use tc_core::task::TaskId;
use tc_executor::traits::{
    ExecutionMode, ExecutionRequest, Executor, SandboxConfig, SandboxPolicy,
};

use crate::error::SpawnError;

pub struct ResolveContext<'a> {
    pub task_id: &'a TaskId,
    pub task_title: &'a str,
    pub worktree: &'a Path,
    pub base_branch: &'a str,
    pub merge_details: &'a str,
    pub config: &'a TcConfig,
}

#[derive(Debug)]
pub enum ResolveOutcome {
    Resolved {
        attempts: usize,
    },
    GaveUp {
        attempts: usize,
        last_details: String,
    },
    Disabled,
}

/// Attempt to resolve a rebase conflict in `worktree` by running the
/// configured agent executor in Yolo + sandbox mode.
///
/// Preconditions: the worktree is currently in "rebase-in-progress" state
/// (i.e. `git rebase <base>` has just failed, the rebase has NOT been
/// aborted, and there are files with conflict markers).
///
/// On `Resolved`, the rebase has been advanced to the end (`git rebase
/// --continue` succeeded) and the caller can proceed with the squash merge.
/// On `GaveUp` or `Disabled`, `git rebase --abort` has been run, leaving the
/// worktree in a clean pre-rebase state -- same shape as the legacy conflict
/// path.
pub async fn try_resolve_rebase_conflict<E: Executor>(
    ctx: ResolveContext<'_>,
    executor: &E,
) -> Result<ResolveOutcome, SpawnError> {
    let resolver = &ctx.config.executor.resolver;
    if !resolver.enabled {
        abort_rebase(ctx.worktree);
        return Ok(ResolveOutcome::Disabled);
    }

    let files = conflicted_files(ctx.worktree)?;
    if files.is_empty() {
        // The merge reported a conflict but there are no unmerged files -- treat
        // as "nothing to do" and fall back to the legacy abort path.
        abort_rebase(ctx.worktree);
        return Ok(ResolveOutcome::GaveUp {
            attempts: 0,
            last_details: "no conflicted files found after reported conflict".into(),
        });
    }

    let prompt = render_prompt(&ctx, &files)?;
    let sandbox = sandbox_from_core(&ctx.config.executor.sandbox);
    let backend = resolver.backend.clone();
    let timeout = Duration::from_secs(resolver.timeout_secs);

    let request = ExecutionRequest {
        context: prompt,
        mode: ExecutionMode::Yolo,
        working_dir: ctx.worktree.to_path_buf(),
        sandbox,
        mcp_servers: vec![],
    };

    let total_attempts = resolver.max_retries.saturating_add(1);
    let mut last_details = String::new();

    'attempts: for attempt in 1..=total_attempts {
        let log_path = resolver_log_path(ctx.config, ctx.task_id, attempt);
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let exec_fut = executor.execute(&request, Some(&log_path));
        let result = tokio::time::timeout(timeout, exec_fut).await;

        match result {
            Err(_elapsed) => {
                abort_rebase(ctx.worktree);
                return Err(SpawnError::ResolverTimeout {
                    backend,
                    secs: resolver.timeout_secs,
                });
            }
            Ok(Err(e)) => {
                last_details = format!("attempt {attempt}: executor error: {e}");
                continue 'attempts;
            }
            Ok(Ok(exec_result)) => {
                if exec_result.exit_code != 0 {
                    last_details = format!(
                        "attempt {attempt}: agent exited with code {}",
                        exec_result.exit_code
                    );
                    continue 'attempts;
                }
            }
        }

        // Check conflict markers BEFORE `git add`. Adding junk with markers
        // would convince git the conflict is resolved, and then the rebase
        // would replay broken content.
        if let Err(reason) = check_no_markers(ctx.worktree, &files) {
            last_details = format!("attempt {attempt}: {reason}");
            if attempt < total_attempts {
                abort_rebase(ctx.worktree);
                restart_rebase(ctx.worktree, ctx.base_branch);
            }
            continue 'attempts;
        }

        // Stage everything and let git evaluate whether all unmerged paths
        // have been resolved.
        if let Err(e) = stage_all(ctx.worktree) {
            last_details = format!("attempt {attempt}: {e}");
            continue 'attempts;
        }

        if let Err(reason) = ensure_no_unmerged(ctx.worktree) {
            last_details = format!("attempt {attempt}: {reason}");
            if attempt < total_attempts {
                abort_rebase(ctx.worktree);
                restart_rebase(ctx.worktree, ctx.base_branch);
            }
            continue 'attempts;
        }

        if let Err(e) = continue_rebase(ctx.worktree) {
            last_details = format!("attempt {attempt}: {e}");
            abort_rebase(ctx.worktree);
            continue 'attempts;
        }
        return Ok(ResolveOutcome::Resolved { attempts: attempt });
    }

    abort_rebase(ctx.worktree);
    Ok(ResolveOutcome::GaveUp {
        attempts: total_attempts,
        last_details,
    })
}

fn render_prompt(ctx: &ResolveContext<'_>, files: &[String]) -> Result<String, SpawnError> {
    let tpl_src = &ctx.config.executor.resolver.template;
    let backend = ctx.config.executor.resolver.backend.clone();

    let mut env = Environment::new();
    env.add_template("resolver", tpl_src)
        .map_err(|e| SpawnError::ResolverTemplate {
            backend: backend.clone(),
            message: e.to_string(),
        })?;
    let tmpl = env
        .get_template("resolver")
        .map_err(|e| SpawnError::ResolverTemplate {
            backend: backend.clone(),
            message: e.to_string(),
        })?;
    let rendered = tmpl
        .render(minijinja::context! {
            id => ctx.task_id.0,
            title => ctx.task_title,
            base_branch => ctx.base_branch,
            worktree => ctx.worktree.display().to_string(),
            merge_details => ctx.merge_details,
            files => files,
        })
        .map_err(|e| SpawnError::ResolverTemplate {
            backend,
            message: e.to_string(),
        })?;
    Ok(rendered)
}

/// List files that git reports as unmerged (conflicted) in `worktree`.
pub fn conflicted_files(worktree: &Path) -> Result<Vec<String>, SpawnError> {
    let output = StdCommand::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(worktree)
        .output()
        .map_err(|e| SpawnError::git("git diff --name-only", e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SpawnError::git("git diff --name-only", stderr.trim()));
    }
    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Ok(files)
}

/// Scan every previously-conflicted file for leftover `<<<<<<<` / `=======` /
/// `>>>>>>>` markers. Done before `git add` so we don't stage broken content.
fn check_no_markers(worktree: &Path, files: &[String]) -> Result<(), String> {
    'scan: for rel in files {
        let path = worktree.join(rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            // File was deleted by the agent; git's unmerged check will catch
            // any real issue downstream.
            continue 'scan;
        };
        if content.contains("<<<<<<<") || content.contains("=======") || content.contains(">>>>>>>")
        {
            return Err(format!("conflict markers still present in {rel}"));
        }
    }
    Ok(())
}

fn stage_all(worktree: &Path) -> Result<(), String> {
    let out = StdCommand::new("git")
        .args(["add", "-A"])
        .current_dir(worktree)
        .output()
        .map_err(|e| format!("git add -A: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git add -A failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn ensure_no_unmerged(worktree: &Path) -> Result<(), String> {
    let out = StdCommand::new("git")
        .args(["ls-files", "--unmerged"])
        .current_dir(worktree)
        .output()
        .map_err(|e| format!("git ls-files --unmerged: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files --unmerged exited with status {}",
            out.status
        ));
    }
    if !out.stdout.is_empty() {
        return Err("git still reports unmerged paths after `git add -A`".into());
    }
    Ok(())
}

fn continue_rebase(worktree: &Path) -> Result<(), String> {
    // `git rebase --continue` may try to open an editor for the commit message.
    // Force a non-interactive commit message by setting GIT_EDITOR=true, which
    // keeps the original author/message.
    let cont = StdCommand::new("git")
        .args(["rebase", "--continue"])
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .current_dir(worktree)
        .output()
        .map_err(|e| format!("git rebase --continue: {e}"))?;
    if !cont.status.success() {
        return Err(format!(
            "git rebase --continue failed: {}",
            String::from_utf8_lossy(&cont.stderr).trim()
        ));
    }
    Ok(())
}

fn abort_rebase(worktree: &Path) {
    let _ = StdCommand::new("git")
        .args(["rebase", "--abort"])
        .current_dir(worktree)
        .output();
}

fn restart_rebase(worktree: &Path, base: &str) {
    let _ = StdCommand::new("git")
        .args(["rebase", base])
        .current_dir(worktree)
        .output();
}

fn sandbox_from_core(core: &tc_core::config::SandboxConfig) -> SandboxConfig {
    let enabled = match core.enabled.as_str() {
        "always" => SandboxPolicy::Always,
        "never" => SandboxPolicy::Never,
        _ => SandboxPolicy::Auto,
    };
    SandboxConfig {
        enabled,
        extra_allow: core.extra_allow.clone(),
        block_network: core.block_network,
    }
}

fn resolver_log_path(config: &TcConfig, task_id: &TaskId, attempt: usize) -> PathBuf {
    // Best effort: walk from spawn base branch to project root via cwd. We
    // don't have Store here, so fall back to ./.tc/logs relative to cwd.
    let _ = config;
    PathBuf::from(".tc/logs").join(format!("resolver-{}-{}.log", task_id.0, attempt))
}
