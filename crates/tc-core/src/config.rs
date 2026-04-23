use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::context::ContextRenderer;
use crate::error::CoreError;
use crate::status::StatusDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcConfig {
    pub statuses: Vec<StatusDef>,
    pub executor: ExecutorConfig,
    pub packer: PackerConfig,
    #[serde(default = "default_context_template")]
    pub context_template: String,
    #[serde(default = "default_plan_template")]
    pub plan_template: String,
    #[serde(default)]
    pub tester: Option<TesterConfig>,
    pub spawn: SpawnConfig,
    #[serde(default)]
    pub verification: VerificationConfig,
}

impl TcConfig {
    /// Validate semantic constraints beyond what serde can check.
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut errors = Vec::new();

        // -- Statuses --
        if self.statuses.is_empty() {
            errors.push(CoreError::invalid_config(
                "statuses",
                "at least one status is required",
            ));
        }
        if !self.statuses.is_empty() && !self.statuses.iter().any(|s| s.terminal) {
            errors.push(CoreError::invalid_config(
                "statuses",
                "at least one terminal status is required",
            ));
        }
        let mut seen_ids = std::collections::HashSet::new();
        for s in &self.statuses {
            if !seen_ids.insert(&s.id) {
                errors.push(CoreError::invalid_config(
                    "statuses",
                    format!("duplicate status id '{}'", s.id),
                ));
            }
        }

        // -- Executor --
        if !matches!(
            self.executor.default.as_str(),
            "claude" | "opencode" | "codex" | "pi" | "gemini" | "all"
        ) {
            errors.push(CoreError::invalid_config(
                "executor.default",
                format!(
                    "unknown executor '{}' (valid: claude, opencode, codex, pi, gemini, all)",
                    self.executor.default
                ),
            ));
        }
        if !matches!(self.executor.mode.as_str(), "accept" | "interactive") {
            errors.push(CoreError::invalid_config(
                "executor.mode",
                format!(
                    "unknown mode '{}' (valid: accept, interactive)",
                    self.executor.mode
                ),
            ));
        }
        if !matches!(
            self.executor.sandbox.enabled.as_str(),
            "auto" | "never" | "always"
        ) {
            errors.push(CoreError::invalid_config(
                "executor.sandbox.enabled",
                format!(
                    "unknown value '{}' (valid: auto, never, always)",
                    self.executor.sandbox.enabled
                ),
            ));
        }

        // -- Packer --
        if self.packer.token_budget == 0 {
            errors.push(CoreError::invalid_config(
                "packer.token_budget",
                "must be > 0",
            ));
        }
        if !matches!(self.packer.style.as_str(), "markdown" | "xml") {
            errors.push(CoreError::invalid_config(
                "packer.style",
                format!(
                    "unknown style '{}' (valid: markdown, xml)",
                    self.packer.style
                ),
            ));
        }

        // -- Spawn --
        if self.spawn.max_parallel == 0 {
            errors.push(CoreError::invalid_config(
                "spawn.max_parallel",
                "must be > 0",
            ));
        }
        if !matches!(self.spawn.isolation.as_str(), "worktree") {
            errors.push(CoreError::invalid_config(
                "spawn.isolation",
                format!(
                    "unknown isolation '{}' (valid: worktree)",
                    self.spawn.isolation
                ),
            ));
        }

        // -- Verification --
        if !self
            .statuses
            .iter()
            .any(|s| s.id.0 == self.verification.on_pass)
            && !self.statuses.is_empty()
        {
            errors.push(CoreError::invalid_config(
                "verification.on_pass",
                format!(
                    "status '{}' not found in statuses",
                    self.verification.on_pass
                ),
            ));
        }
        if !self
            .statuses
            .iter()
            .any(|s| s.id.0 == self.verification.on_fail)
            && !self.statuses.is_empty()
        {
            errors.push(CoreError::invalid_config(
                "verification.on_fail",
                format!(
                    "status '{}' not found in statuses",
                    self.verification.on_fail
                ),
            ));
        }

        // -- Context template --
        if let Err(e) = ContextRenderer::new(&self.context_template) {
            errors.push(CoreError::invalid_config(
                "context_template",
                format!("{e}"),
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(CoreError::validation(errors))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutorConfig {
    #[serde(default = "default_executor")]
    pub default: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub resolver: ResolverConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomBackendConfig {
    pub command: String,
    #[serde(default)]
    pub yolo_args: Vec<String>,
    #[serde(default)]
    pub accept_args: Vec<String>,
    #[serde(default)]
    pub interactive_args: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolverBackends {
    #[serde(default)]
    pub codex: Option<CustomBackendConfig>,
    #[serde(default)]
    pub pi: Option<CustomBackendConfig>,
    #[serde(default)]
    pub gemini: Option<CustomBackendConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_resolver_backend")]
    pub backend: String,
    #[serde(default = "default_resolver_template")]
    pub template: String,
    #[serde(default = "default_resolver_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_resolver_max_retries")]
    pub max_retries: usize,
    #[serde(default)]
    pub backends: ResolverBackends,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: default_resolver_backend(),
            template: default_resolver_template(),
            timeout_secs: default_resolver_timeout(),
            max_retries: default_resolver_max_retries(),
            backends: ResolverBackends::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(default = "default_sandbox_enabled")]
    pub enabled: String,
    #[serde(default)]
    pub extra_allow: Vec<PathBuf>,
    #[serde(default)]
    pub block_network: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackerConfig {
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
    #[serde(default = "default_pack_style")]
    pub style: String,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TesterConfig {
    #[serde(default = "default_executor")]
    pub executor: String,
    #[serde(default)]
    pub mcp: Vec<McpServerConfig>,
    #[serde(default)]
    pub system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnConfig {
    #[serde(default = "default_max_parallel")]
    pub max_parallel: usize,
    #[serde(default = "default_isolation")]
    pub isolation: String,
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
    #[serde(default = "default_branch_prefix")]
    pub branch_prefix: String,
    #[serde(default)]
    pub auto_commit: bool,
    #[serde(default = "default_on_complete")]
    pub on_complete: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default = "default_on_pass")]
    pub on_pass: String,
    #[serde(default = "default_on_fail")]
    pub on_fail: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
    #[serde(default)]
    pub auto_retry: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            commands: vec![],
            on_pass: default_on_pass(),
            on_fail: default_on_fail(),
            max_retries: default_max_retries(),
            auto_retry: false,
        }
    }
}

fn default_on_pass() -> String {
    "review".into()
}
fn default_on_fail() -> String {
    "blocked".into()
}
fn default_max_retries() -> usize {
    0
}

fn default_executor() -> String {
    "claude".into()
}
fn default_mode() -> String {
    "accept".into()
}
fn default_sandbox_enabled() -> String {
    "auto".into()
}
fn default_token_budget() -> usize {
    80_000
}
fn default_pack_style() -> String {
    "markdown".into()
}
fn default_max_parallel() -> usize {
    3
}
fn default_isolation() -> String {
    "worktree".into()
}
fn default_base_branch() -> String {
    "main".into()
}
fn default_branch_prefix() -> String {
    "tc/".into()
}
fn default_resolver_backend() -> String {
    "claude".into()
}
fn default_resolver_timeout() -> u64 {
    300
}
fn default_resolver_max_retries() -> usize {
    1
}
fn default_resolver_template() -> String {
    r#"You are resolving a git rebase conflict in a worktree.

Task: {{ id }} -- {{ title }}
Base branch: {{ base_branch }}
Worktree: {{ worktree }}

Conflicted files:
{% for f in files %}- {{ f }}
{% endfor %}

Git output:
{{ merge_details }}

Resolve every conflict so the rebase can continue:
1. Edit each file to keep both sides' intent; remove all `<<<<<<<`, `=======`, `>>>>>>>` markers.
2. Do not run `git add` or `git rebase --continue` yourself -- tc handles that.
3. If a conflict is ambiguous or dangerous, exit with a non-zero code and leave markers in place.
"#
    .into()
}
fn default_on_complete() -> String {
    "pr".into()
}
fn default_context_template() -> String {
    r#"# Task {{ id }}: {{ title }}

**Epic:** {{ epic }}
**Dependencies (done):** {{ resolved_deps }}

{% if acceptance_criteria %}
## Acceptance Criteria
{{ acceptance_criteria }}
{% endif %}

## Notes
{{ notes }}

{% if packed_files %}
## Relevant Files
{{ packed_files }}
{% endif %}

## Checklist
- [ ] Implement the task
- [ ] Run tests
- [ ] Mark as done: `tc done {{ id }}`
"#
    .into()
}

fn default_plan_template() -> String {
    r#"You are a senior software architect. Analyze the codebase and produce an implementation plan for the following task. Do NOT implement anything -- only plan.

# Task {{ id }}: {{ title }}

**Epic:** {{ epic }}
**Dependencies (done):** {{ resolved_deps }}

{% if acceptance_criteria %}
## Acceptance Criteria
{{ acceptance_criteria }}
{% endif %}

## Notes
{{ notes }}

{% if packed_files %}
## Relevant Files
{{ packed_files }}
{% endif %}

## Instructions

1. Read the relevant source files to understand the current architecture.
2. Identify every file that needs to be created or modified.
3. For each file, describe the specific changes (functions, structs, traits, etc.).
4. Call out edge cases, error handling, and testing strategy.
5. Suggest an implementation order respecting dependencies.

Output a structured plan in Markdown.
"#
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::StatusId;

    fn valid_config() -> TcConfig {
        TcConfig {
            statuses: vec![
                StatusDef {
                    id: StatusId("todo".into()),
                    label: "Todo".into(),
                    terminal: false,
                },
                StatusDef {
                    id: StatusId("review".into()),
                    label: "Review".into(),
                    terminal: false,
                },
                StatusDef {
                    id: StatusId("done".into()),
                    label: "Done".into(),
                    terminal: true,
                },
                StatusDef {
                    id: StatusId("blocked".into()),
                    label: "Blocked".into(),
                    terminal: false,
                },
            ],
            executor: ExecutorConfig {
                default: "claude".into(),
                mode: "accept".into(),
                sandbox: SandboxConfig {
                    enabled: "auto".into(),
                    extra_allow: vec![],
                    block_network: false,
                },
                resolver: ResolverConfig::default(),
            },
            packer: PackerConfig {
                token_budget: 80_000,
                style: "markdown".into(),
                ignore_patterns: vec![],
            },
            context_template: default_context_template(),
            plan_template: default_plan_template(),
            tester: None,
            spawn: SpawnConfig {
                max_parallel: 3,
                isolation: "worktree".into(),
                base_branch: "main".into(),
                branch_prefix: "tc/".into(),
                auto_commit: false,
                on_complete: "pr".into(),
            },
            verification: VerificationConfig::default(),
        }
    }

    #[test]
    fn valid_config_passes() {
        valid_config().validate().unwrap();
    }

    #[test]
    fn empty_statuses_fails() {
        let mut cfg = valid_config();
        cfg.statuses = vec![];
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn no_terminal_status_fails() {
        let mut cfg = valid_config();
        for s in &mut cfg.statuses {
            s.terminal = false;
        }
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn duplicate_status_id_fails() {
        let mut cfg = valid_config();
        cfg.statuses.push(StatusDef {
            id: StatusId("todo".into()),
            label: "Duplicate".into(),
            terminal: false,
        });
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn invalid_executor_fails() {
        let mut cfg = valid_config();
        cfg.executor.default = "vim".into();
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn invalid_mode_fails() {
        let mut cfg = valid_config();
        cfg.executor.mode = "yolo".into();
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn invalid_sandbox_enabled_fails() {
        let mut cfg = valid_config();
        cfg.executor.sandbox.enabled = "maybe".into();
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn zero_token_budget_fails() {
        let mut cfg = valid_config();
        cfg.packer.token_budget = 0;
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn invalid_pack_style_fails() {
        let mut cfg = valid_config();
        cfg.packer.style = "json".into();
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn zero_max_parallel_fails() {
        let mut cfg = valid_config();
        cfg.spawn.max_parallel = 0;
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn invalid_isolation_fails() {
        let mut cfg = valid_config();
        cfg.spawn.isolation = "docker".into();
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn invalid_verification_on_pass_status_fails() {
        let mut cfg = valid_config();
        cfg.verification.on_pass = "nonexistent".into();
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn invalid_context_template_fails() {
        let mut cfg = valid_config();
        cfg.context_template = "{{ unclosed".into();
        let err = cfg.validate().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("error"), "{msg}");
    }

    #[test]
    fn opencode_executor_valid() {
        let mut cfg = valid_config();
        cfg.executor.default = "opencode".into();
        cfg.validate().unwrap();
    }

    #[test]
    fn gemini_executor_valid() {
        let mut cfg = valid_config();
        cfg.executor.default = "gemini".into();
        cfg.validate().unwrap();
    }

    #[test]
    fn all_executor_valid() {
        let mut cfg = valid_config();
        cfg.executor.default = "all".into();
        cfg.validate().unwrap();
    }

    #[test]
    fn xml_pack_style_valid() {
        let mut cfg = valid_config();
        cfg.packer.style = "xml".into();
        cfg.validate().unwrap();
    }

    #[test]
    fn interactive_mode_valid() {
        let mut cfg = valid_config();
        cfg.executor.mode = "interactive".into();
        cfg.validate().unwrap();
    }
}
