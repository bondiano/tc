use std::path::Path;

use tc_core::config::TcConfig;

use crate::claude::ClaudeExecutor;
use crate::custom::CustomExecutor;
use crate::error::ExecutorError;
use crate::opencode::OpencodeExecutor;
use crate::traits::{ExecutionRequest, ExecutionResult, Executor};

/// Dynamic-dispatch wrapper over the concrete executor types.
///
/// The `Executor` trait uses return-position `impl Future` which is not
/// object-safe, so we can't store `Box<dyn Executor>`. This enum covers the
/// fixed set of backends we ship and delegates to the right concrete type.
pub enum AnyExecutor {
    Claude(ClaudeExecutor),
    Opencode(OpencodeExecutor),
    Custom(CustomExecutor),
}

impl AnyExecutor {
    pub fn name(&self) -> &str {
        match self {
            Self::Claude(_) => "claude",
            Self::Opencode(_) => "opencode",
            Self::Custom(e) => e.name(),
        }
    }
}

/// Build an `AnyExecutor` by name, resolving custom backends from `TcConfig`.
///
/// Errors:
/// - `NotFound` if `name` is not one of claude/opencode/codex/pi/gemini.
/// - `Sandbox` (repurposed as a config error) if a custom backend has no
///   command configured -- validation should have caught this earlier, but we
///   double-check to keep the error close to the actual invocation.
pub fn executor_by_name(name: &str, cfg: &TcConfig) -> Result<AnyExecutor, ExecutorError> {
    match name {
        "claude" => Ok(AnyExecutor::Claude(ClaudeExecutor)),
        "opencode" => Ok(AnyExecutor::Opencode(OpencodeExecutor)),
        "codex" => {
            let c = cfg
                .executor
                .resolver
                .backends
                .codex
                .as_ref()
                .ok_or_else(|| ExecutorError::sandbox("codex backend missing command config"))?;
            Ok(AnyExecutor::Custom(CustomExecutor::codex(c.clone())))
        }
        "pi" => {
            let p = cfg
                .executor
                .resolver
                .backends
                .pi
                .as_ref()
                .ok_or_else(|| ExecutorError::sandbox("pi backend missing command config"))?;
            Ok(AnyExecutor::Custom(CustomExecutor::pi(p.clone())))
        }
        "gemini" => {
            let g = cfg
                .executor
                .resolver
                .backends
                .gemini
                .as_ref()
                .ok_or_else(|| ExecutorError::sandbox("gemini backend missing command config"))?;
            Ok(AnyExecutor::Custom(CustomExecutor::gemini(g.clone())))
        }
        _ => Err(ExecutorError::not_found(name)),
    }
}

impl Executor for AnyExecutor {
    fn build_command(
        &self,
        request: &ExecutionRequest,
    ) -> Result<tokio::process::Command, ExecutorError> {
        match self {
            Self::Claude(e) => e.build_command(request),
            Self::Opencode(e) => e.build_command(request),
            Self::Custom(e) => e.build_command(request),
        }
    }

    async fn execute(
        &self,
        request: &ExecutionRequest,
        log_sink: Option<&Path>,
    ) -> Result<ExecutionResult, ExecutorError> {
        match self {
            Self::Claude(e) => e.execute(request, log_sink).await,
            Self::Opencode(e) => e.execute(request, log_sink).await,
            Self::Custom(e) => e.execute(request, log_sink).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_core::config::{CustomBackendConfig, ResolverConfig, TcConfig};

    fn cfg_with_resolver(resolver: ResolverConfig) -> TcConfig {
        let mut cfg = minimal_cfg();
        cfg.executor.resolver = resolver;
        cfg
    }

    fn minimal_cfg() -> TcConfig {
        use tc_core::config::{
            ExecutorConfig, PackerConfig, SandboxConfig, SpawnConfig, VerificationConfig,
        };
        use tc_core::status::{StatusDef, StatusId};
        TcConfig {
            statuses: vec![
                StatusDef {
                    id: StatusId("todo".into()),
                    label: "Todo".into(),
                    terminal: false,
                },
                StatusDef {
                    id: StatusId("done".into()),
                    label: "Done".into(),
                    terminal: true,
                },
            ],
            executor: ExecutorConfig {
                default: "claude".into(),
                mode: "accept".into(),
                sandbox: SandboxConfig::default(),
                resolver: ResolverConfig::default(),
            },
            packer: PackerConfig {
                token_budget: 80_000,
                style: "markdown".into(),
                ignore_patterns: vec![],
            },
            context_template: "{{ id }}".into(),
            plan_template: "{{ id }}".into(),
            tester: None,
            spawn: SpawnConfig {
                max_parallel: 1,
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
    fn claude_by_name() {
        let cfg = minimal_cfg();
        let exec = executor_by_name("claude", &cfg).unwrap();
        assert_eq!(exec.name(), "claude");
    }

    #[test]
    fn opencode_by_name() {
        let cfg = minimal_cfg();
        let exec = executor_by_name("opencode", &cfg).unwrap();
        assert_eq!(exec.name(), "opencode");
    }

    #[test]
    fn codex_without_backend_errors() {
        let cfg = minimal_cfg();
        let result = executor_by_name("codex", &cfg);
        assert!(matches!(result, Err(ExecutorError::Sandbox { .. })));
    }

    #[test]
    fn codex_with_backend_works() {
        let mut resolver = ResolverConfig::default();
        resolver.backends.codex = Some(CustomBackendConfig {
            command: "codex".into(),
            yolo_args: vec!["{context}".into()],
            accept_args: vec![],
            interactive_args: vec![],
        });
        let cfg = cfg_with_resolver(resolver);
        let exec = executor_by_name("codex", &cfg).unwrap();
        assert_eq!(exec.name(), "codex");
    }

    #[test]
    fn gemini_without_backend_errors() {
        let cfg = minimal_cfg();
        let result = executor_by_name("gemini", &cfg);
        assert!(matches!(result, Err(ExecutorError::Sandbox { .. })));
    }

    #[test]
    fn gemini_with_backend_works() {
        let mut resolver = ResolverConfig::default();
        resolver.backends.gemini = Some(CustomBackendConfig {
            command: "gemini".into(),
            yolo_args: vec!["--prompt".into(), "{context}".into()],
            accept_args: vec![],
            interactive_args: vec![],
        });
        let cfg = cfg_with_resolver(resolver);
        let exec = executor_by_name("gemini", &cfg).unwrap();
        assert_eq!(exec.name(), "gemini");
    }

    #[test]
    fn unknown_name_errors() {
        let cfg = minimal_cfg();
        let result = executor_by_name("vim", &cfg);
        assert!(matches!(result, Err(ExecutorError::NotFound { .. })));
    }
}
