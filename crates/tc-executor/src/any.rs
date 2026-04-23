use std::path::Path;

use tc_core::config::{ExecutorKind, TcConfig};

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

/// Resolve the PATH command that `kind` would invoke on this system.
///
/// For `Claude`/`Opencode`, the command is the kind name itself.
/// For custom backends it comes from `cfg.executor.resolver.backends.*`;
/// if nothing is configured we fall back to the kind name so PATH lookup
/// still has something to try.
pub fn command_for(kind: ExecutorKind, cfg: &TcConfig) -> String {
    match kind {
        ExecutorKind::Claude | ExecutorKind::Opencode => kind.as_str().to_string(),
        ExecutorKind::Codex => cfg
            .executor
            .resolver
            .backends
            .codex
            .as_ref()
            .map(|c| c.command.clone())
            .unwrap_or_else(|| kind.as_str().to_string()),
        ExecutorKind::Pi => cfg
            .executor
            .resolver
            .backends
            .pi
            .as_ref()
            .map(|c| c.command.clone())
            .unwrap_or_else(|| kind.as_str().to_string()),
        ExecutorKind::Gemini => cfg
            .executor
            .resolver
            .backends
            .gemini
            .as_ref()
            .map(|c| c.command.clone())
            .unwrap_or_else(|| kind.as_str().to_string()),
    }
}

/// Report whether the binary backing `kind` exists on PATH.
pub fn is_installed(kind: ExecutorKind, cfg: &TcConfig) -> bool {
    which::which(command_for(kind, cfg)).is_ok()
}

/// Build an `AnyExecutor` for the given kind, resolving custom backends
/// from `TcConfig`.
///
/// Errors with `Sandbox` (re-used as a config error) when a custom backend
/// has no command configured -- validation should catch it earlier, but we
/// double-check here so the error is close to the actual invocation.
pub fn executor_by_kind(kind: ExecutorKind, cfg: &TcConfig) -> Result<AnyExecutor, ExecutorError> {
    match kind {
        ExecutorKind::Claude => Ok(AnyExecutor::Claude(ClaudeExecutor)),
        ExecutorKind::Opencode => Ok(AnyExecutor::Opencode(OpencodeExecutor)),
        ExecutorKind::Codex => {
            let c = cfg
                .executor
                .resolver
                .backends
                .codex
                .as_ref()
                .ok_or_else(|| ExecutorError::sandbox("codex backend missing command config"))?;
            Ok(AnyExecutor::Custom(CustomExecutor::codex(c.clone())))
        }
        ExecutorKind::Pi => {
            let p = cfg
                .executor
                .resolver
                .backends
                .pi
                .as_ref()
                .ok_or_else(|| ExecutorError::sandbox("pi backend missing command config"))?;
            Ok(AnyExecutor::Custom(CustomExecutor::pi(p.clone())))
        }
        ExecutorKind::Gemini => {
            let g = cfg
                .executor
                .resolver
                .backends
                .gemini
                .as_ref()
                .ok_or_else(|| ExecutorError::sandbox("gemini backend missing command config"))?;
            Ok(AnyExecutor::Custom(CustomExecutor::gemini(g.clone())))
        }
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
            ExecutionMode, ExecutorConfig, PackerConfig, SandboxConfig, SpawnConfig,
            VerificationConfig,
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
                default: ExecutorKind::Claude,
                mode: ExecutionMode::Accept,
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
    fn claude_by_kind() {
        let cfg = minimal_cfg();
        let exec = executor_by_kind(ExecutorKind::Claude, &cfg).unwrap();
        assert_eq!(exec.name(), "claude");
    }

    #[test]
    fn opencode_by_kind() {
        let cfg = minimal_cfg();
        let exec = executor_by_kind(ExecutorKind::Opencode, &cfg).unwrap();
        assert_eq!(exec.name(), "opencode");
    }

    #[test]
    fn codex_without_backend_errors() {
        let cfg = minimal_cfg();
        let result = executor_by_kind(ExecutorKind::Codex, &cfg);
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
        let exec = executor_by_kind(ExecutorKind::Codex, &cfg).unwrap();
        assert_eq!(exec.name(), "codex");
    }

    #[test]
    fn gemini_without_backend_errors() {
        let cfg = minimal_cfg();
        let result = executor_by_kind(ExecutorKind::Gemini, &cfg);
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
        let exec = executor_by_kind(ExecutorKind::Gemini, &cfg).unwrap();
        assert_eq!(exec.name(), "gemini");
    }

    #[test]
    fn command_for_custom_without_config_falls_back_to_kind_name() {
        let cfg = minimal_cfg();
        assert_eq!(command_for(ExecutorKind::Codex, &cfg), "codex");
    }

    #[test]
    fn command_for_custom_uses_configured_command() {
        let mut resolver = ResolverConfig::default();
        resolver.backends.pi = Some(CustomBackendConfig {
            command: "/opt/bin/pi".into(),
            yolo_args: vec![],
            accept_args: vec![],
            interactive_args: vec![],
        });
        let cfg = cfg_with_resolver(resolver);
        assert_eq!(command_for(ExecutorKind::Pi, &cfg), "/opt/bin/pi");
    }
}
