use std::path::Path;

use crate::traits::{SandboxConfig, SandboxPolicy};

/// Build an executor `SandboxConfig` from the core/user-facing config.
///
/// Maps the stringly-typed `enabled` field to a `SandboxPolicy`.
pub fn sandbox_from_core(core: &tc_core::config::SandboxConfig) -> SandboxConfig {
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

/// Detected sandbox provider. Ordered roughly by preference across OSes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxProvider {
    /// Docker AI Sandboxes (MicroVM isolation, cross-platform).
    Sbx,
    /// Landlock kernel-level FS restrictions (Linux only).
    Nono,
    /// macOS built-in `sandbox-exec` (seatbelt) fallback.
    SandboxExec,
    /// bubblewrap user-namespace sandbox (Linux fallback).
    Bwrap,
    /// No sandbox available -- run without isolation (last resort).
    None,
}

impl SandboxProvider {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Sbx => "sbx",
            Self::Nono => "nono",
            Self::SandboxExec => "sandbox-exec",
            Self::Bwrap => "bwrap",
            Self::None => "none",
        }
    }
}

/// Detect the best available sandbox provider based on config and PATH.
///
/// Priority: sbx > nono > sandbox-exec (macOS) > bwrap (Linux) > none.
pub fn detect_provider(config: &SandboxConfig) -> SandboxProvider {
    if matches!(config.enabled, SandboxPolicy::Never) {
        return SandboxProvider::None;
    }
    if detect_sbx() {
        return SandboxProvider::Sbx;
    }
    if detect_nono() {
        return SandboxProvider::Nono;
    }
    if detect_sandbox_exec() {
        return SandboxProvider::SandboxExec;
    }
    if detect_bwrap() {
        return SandboxProvider::Bwrap;
    }
    SandboxProvider::None
}

/// Wrap a command with the appropriate sandbox provider.
///
/// Returns the command program and args, prefixed with sandbox invocation.
pub fn wrap_with_sandbox(
    program: &str,
    args: &[String],
    provider: &SandboxProvider,
    config: &SandboxConfig,
    working_dir: &Path,
) -> (String, Vec<String>) {
    match provider {
        SandboxProvider::Sbx => wrap_sbx(program, args, config),
        SandboxProvider::Nono => wrap_nono(program, args, config, working_dir),
        SandboxProvider::SandboxExec => wrap_sandbox_exec(program, args, config, working_dir),
        SandboxProvider::Bwrap => wrap_bwrap(program, args, config, working_dir),
        SandboxProvider::None => (program.to_string(), args.to_vec()),
    }
}

fn wrap_sbx(program: &str, args: &[String], _config: &SandboxConfig) -> (String, Vec<String>) {
    let mut sbx_args = vec!["run".to_string(), program.to_string(), "--".to_string()];
    sbx_args.extend_from_slice(args);
    ("sbx".to_string(), sbx_args)
}

fn wrap_nono(
    program: &str,
    args: &[String],
    config: &SandboxConfig,
    working_dir: &Path,
) -> (String, Vec<String>) {
    let mut nono_args = vec![
        "run".to_string(),
        "--allow".to_string(),
        working_dir.display().to_string(),
    ];

    for extra in &config.extra_allow {
        nono_args.push("--allow".to_string());
        nono_args.push(extra.display().to_string());
    }

    nono_args.push("--".to_string());
    nono_args.push(program.to_string());
    nono_args.extend_from_slice(args);

    ("nono".to_string(), nono_args)
}

fn wrap_sandbox_exec(
    program: &str,
    args: &[String],
    config: &SandboxConfig,
    working_dir: &Path,
) -> (String, Vec<String>) {
    let profile = build_sandbox_exec_profile(working_dir, config);
    let mut se_args = vec!["-p".to_string(), profile, program.to_string()];
    se_args.extend_from_slice(args);
    ("sandbox-exec".to_string(), se_args)
}

fn build_sandbox_exec_profile(working_dir: &Path, config: &SandboxConfig) -> String {
    let mut profile = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");

    let mut allow_write = |path: &str| {
        profile.push_str(&format!("(allow file-write* (subpath \"{path}\"))\n"));
    };

    allow_write(&working_dir.display().to_string());
    allow_write("/private/tmp");
    allow_write("/private/var/folders");
    allow_write("/private/var/tmp");
    allow_write("/tmp");

    for extra in &config.extra_allow {
        allow_write(&extra.display().to_string());
    }

    if config.block_network {
        profile.push_str("(deny network*)\n");
    }

    profile
}

fn wrap_bwrap(
    program: &str,
    args: &[String],
    config: &SandboxConfig,
    working_dir: &Path,
) -> (String, Vec<String>) {
    let wd = working_dir.display().to_string();
    let mut bwrap_args = vec![
        "--ro-bind".to_string(),
        "/".to_string(),
        "/".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--bind".to_string(),
        wd.clone(),
        wd.clone(),
        "--chdir".to_string(),
        wd,
    ];

    for extra in &config.extra_allow {
        let p = extra.display().to_string();
        bwrap_args.push("--bind".to_string());
        bwrap_args.push(p.clone());
        bwrap_args.push(p);
    }

    if config.block_network {
        bwrap_args.push("--unshare-net".to_string());
    }

    bwrap_args.push("--".to_string());
    bwrap_args.push(program.to_string());
    bwrap_args.extend_from_slice(args);

    ("bwrap".to_string(), bwrap_args)
}

pub fn detect_sbx() -> bool {
    which::which("sbx").is_ok()
}

pub fn detect_nono() -> bool {
    cfg!(target_os = "linux") && which::which("nono").is_ok()
}

pub fn detect_sandbox_exec() -> bool {
    cfg!(target_os = "macos") && which::which("sandbox-exec").is_ok()
}

pub fn detect_bwrap() -> bool {
    cfg!(target_os = "linux") && which::which("bwrap").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn default_config() -> SandboxConfig {
        SandboxConfig {
            enabled: SandboxPolicy::Auto,
            extra_allow: vec![],
            block_network: false,
        }
    }

    #[test]
    fn detect_provider_never_returns_none() {
        let config = SandboxConfig {
            enabled: SandboxPolicy::Never,
            ..default_config()
        };
        assert_eq!(detect_provider(&config), SandboxProvider::None);
    }

    #[test]
    fn wrap_none_passthrough() {
        let (prog, args) = wrap_with_sandbox(
            "claude",
            &["--print".to_string(), "hello".to_string()],
            &SandboxProvider::None,
            &default_config(),
            Path::new("/work"),
        );
        assert_eq!(prog, "claude");
        assert_eq!(args, vec!["--print", "hello"]);
    }

    #[test]
    fn wrap_sbx_format() {
        let (prog, args) = wrap_with_sandbox(
            "claude",
            &["--print".to_string(), "context".to_string()],
            &SandboxProvider::Sbx,
            &default_config(),
            Path::new("/work"),
        );
        assert_eq!(prog, "sbx");
        assert_eq!(args, vec!["run", "claude", "--", "--print", "context"]);
    }

    #[test]
    fn wrap_nono_format() {
        let (prog, args) = wrap_with_sandbox(
            "claude",
            &["--print".to_string(), "context".to_string()],
            &SandboxProvider::Nono,
            &default_config(),
            Path::new("/work/project"),
        );
        assert_eq!(prog, "nono");
        assert_eq!(
            args,
            vec![
                "run",
                "--allow",
                "/work/project",
                "--",
                "claude",
                "--print",
                "context"
            ]
        );
    }

    #[test]
    fn wrap_nono_with_extra_allow() {
        let config = SandboxConfig {
            enabled: SandboxPolicy::Auto,
            extra_allow: vec![PathBuf::from("/tmp"), PathBuf::from("/home/user/.cache")],
            block_network: false,
        };
        let (prog, args) = wrap_with_sandbox(
            "claude",
            &[],
            &SandboxProvider::Nono,
            &config,
            Path::new("/work"),
        );
        assert_eq!(prog, "nono");
        assert!(args.contains(&"--allow".to_string()));
        assert!(args.contains(&"/tmp".to_string()));
        assert!(args.contains(&"/home/user/.cache".to_string()));
    }

    #[test]
    fn wrap_sandbox_exec_format() {
        let (prog, args) = wrap_with_sandbox(
            "claude",
            &["--print".to_string(), "context".to_string()],
            &SandboxProvider::SandboxExec,
            &default_config(),
            Path::new("/work/project"),
        );
        assert_eq!(prog, "sandbox-exec");
        assert_eq!(args[0], "-p");
        assert!(args[1].contains("(version 1)"));
        assert!(args[1].contains("/work/project"));
        assert_eq!(args[2], "claude");
        assert_eq!(&args[3..], &["--print", "context"]);
    }

    #[test]
    fn sandbox_exec_profile_includes_block_network() {
        let config = SandboxConfig {
            enabled: SandboxPolicy::Auto,
            extra_allow: vec![PathBuf::from("/opt/cache")],
            block_network: true,
        };
        let profile = build_sandbox_exec_profile(Path::new("/work"), &config);
        assert!(profile.contains("(deny file-write*)"));
        assert!(profile.contains("(subpath \"/work\")"));
        assert!(profile.contains("(subpath \"/opt/cache\")"));
        assert!(profile.contains("(deny network*)"));
    }

    #[test]
    fn sandbox_exec_profile_excludes_network_deny_by_default() {
        let profile = build_sandbox_exec_profile(Path::new("/work"), &default_config());
        assert!(!profile.contains("(deny network*)"));
    }

    #[test]
    fn wrap_bwrap_binds_working_dir_rw() {
        let (prog, args) = wrap_with_sandbox(
            "claude",
            &["--print".to_string()],
            &SandboxProvider::Bwrap,
            &default_config(),
            Path::new("/work/project"),
        );
        assert_eq!(prog, "bwrap");
        let joined = args.join(" ");
        assert!(joined.contains("--ro-bind / /"));
        assert!(joined.contains("--bind /work/project /work/project"));
        assert!(joined.contains("--chdir /work/project"));
        assert!(joined.ends_with("-- claude --print"));
    }

    #[test]
    fn wrap_bwrap_block_network_adds_unshare() {
        let config = SandboxConfig {
            enabled: SandboxPolicy::Auto,
            extra_allow: vec![],
            block_network: true,
        };
        let (_, args) = wrap_with_sandbox(
            "claude",
            &[],
            &SandboxProvider::Bwrap,
            &config,
            Path::new("/work"),
        );
        assert!(args.contains(&"--unshare-net".to_string()));
    }

    #[test]
    fn provider_variants_are_distinct() {
        assert_ne!(SandboxProvider::Sbx, SandboxProvider::Nono);
        assert_ne!(SandboxProvider::Nono, SandboxProvider::None);
        assert_ne!(SandboxProvider::Sbx, SandboxProvider::None);
        assert_ne!(SandboxProvider::SandboxExec, SandboxProvider::Bwrap);
    }

    #[test]
    fn provider_name_is_stable() {
        assert_eq!(SandboxProvider::Sbx.name(), "sbx");
        assert_eq!(SandboxProvider::Nono.name(), "nono");
        assert_eq!(SandboxProvider::SandboxExec.name(), "sandbox-exec");
        assert_eq!(SandboxProvider::Bwrap.name(), "bwrap");
        assert_eq!(SandboxProvider::None.name(), "none");
    }
}
