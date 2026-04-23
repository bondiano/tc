use std::process::Command;

use crate::cli::{ConfigAction, ConfigArgs};
use crate::error::CliError;
use crate::output;

pub fn run(args: ConfigArgs) -> Result<(), CliError> {
    match args.action {
        None | Some(ConfigAction::List) => run_list(),
        Some(ConfigAction::Get { key }) => run_get(&key),
        Some(ConfigAction::Set { key, value }) => run_set(&key, &value),
        Some(ConfigAction::Edit) => run_edit(),
        Some(ConfigAction::Path) => run_path(),
        Some(ConfigAction::Reset) => run_reset(),
    }
}

fn run_list() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let content = std::fs::read_to_string(store.config_path())
        .map_err(|e| CliError::user(format!("Failed to read config: {e}")))?;
    print!("{content}");
    Ok(())
}

fn run_get(key: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let content = std::fs::read_to_string(store.config_path())
        .map_err(|e| CliError::user(format!("Failed to read config: {e}")))?;
    let root: serde_yaml_ng::Value = serde_yaml_ng::from_str(&content)
        .map_err(|e| CliError::user(format!("Failed to parse config: {e}")))?;

    let value =
        resolve_path(&root, key).ok_or_else(|| CliError::user(format!("Key not found: {key}")))?;

    println!("{}", format_value(value));
    Ok(())
}

fn run_set(key: &str, raw_value: &str) -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let content = std::fs::read_to_string(store.config_path())
        .map_err(|e| CliError::user(format!("Failed to read config: {e}")))?;
    let mut root: serde_yaml_ng::Value = serde_yaml_ng::from_str(&content)
        .map_err(|e| CliError::user(format!("Failed to parse config: {e}")))?;

    let parsed_value = parse_value(raw_value);

    set_path(&mut root, key, parsed_value).map_err(CliError::user)?;

    // Validate by deserializing into TcConfig and running semantic checks
    let yaml_str = serde_yaml_ng::to_string(&root)
        .map_err(|e| CliError::user(format!("Failed to serialize config: {e}")))?;
    let config: tc_core::config::TcConfig = serde_yaml_ng::from_str(&yaml_str)
        .map_err(|e| CliError::user(format!("Invalid config after update: {e}")))?;
    config
        .validate()
        .map_err(|e| CliError::user(format!("Invalid config after update: {e}")))?;

    std::fs::write(store.config_path(), &yaml_str)
        .map_err(|e| CliError::user(format!("Failed to write config: {e}")))?;

    output::print_success(&format!("{key} = {raw_value}"));
    Ok(())
}

fn run_edit() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let config_path = store.config_path();

    let before = std::fs::read_to_string(&config_path)
        .map_err(|e| CliError::user(format!("Failed to read config: {e}")))?;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|e| CliError::user(format!("Failed to launch editor '{editor}': {e}")))?;

    if !status.success() {
        return Err(CliError::user(format!(
            "Editor exited with status: {status}"
        )));
    }

    let after = std::fs::read_to_string(&config_path)
        .map_err(|e| CliError::user(format!("Failed to read config: {e}")))?;

    if after == before {
        output::print_warning("No changes made");
        return Ok(());
    }

    // Validate the edited config (YAML syntax + semantic checks)
    let config: tc_core::config::TcConfig = serde_yaml_ng::from_str(&after).map_err(|e| {
        let _ = std::fs::write(&config_path, &before);
        CliError::user(format!("Invalid config (reverted): {e}"))
    })?;
    config.validate().map_err(|e| {
        let _ = std::fs::write(&config_path, &before);
        CliError::user(format!("Invalid config (reverted): {e}"))
    })?;

    output::print_success("Config updated");
    Ok(())
}

fn run_path() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    println!("{}", store.config_path().display());
    Ok(())
}

fn run_reset() -> Result<(), CliError> {
    let store = tc_storage::Store::discover()?;
    let default_config = tc_storage::init::default_config();
    std::fs::write(store.config_path(), default_config)
        .map_err(|e| CliError::user(format!("Failed to write config: {e}")))?;
    output::print_success("Config reset to defaults");
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────

fn resolve_path<'a>(
    root: &'a serde_yaml_ng::Value,
    path: &str,
) -> Option<&'a serde_yaml_ng::Value> {
    let mut current = root;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

fn set_path(
    root: &mut serde_yaml_ng::Value,
    path: &str,
    value: serde_yaml_ng::Value,
) -> Result<(), String> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        return Err("Empty key path".into());
    }

    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        current = current
            .get_mut(*segment)
            .ok_or_else(|| format!("Key not found: {segment}"))?;
    }

    let last = segments[segments.len() - 1];
    match current {
        serde_yaml_ng::Value::Mapping(map) => {
            let key = serde_yaml_ng::Value::String(last.to_string());
            if !map.contains_key(&key) {
                return Err(format!("Key not found: {last}"));
            }
            map.insert(key, value);
            Ok(())
        }
        _ => Err(format!(
            "Cannot set key on non-mapping value at '{}'",
            segments[..segments.len() - 1].join(".")
        )),
    }
}

fn parse_value(raw: &str) -> serde_yaml_ng::Value {
    // Try bool
    match raw {
        "true" => return serde_yaml_ng::Value::Bool(true),
        "false" => return serde_yaml_ng::Value::Bool(false),
        _ => {}
    }
    // Try integer
    if let Ok(n) = raw.parse::<i64>() {
        return serde_yaml_ng::Value::Number(n.into());
    }
    // Try float
    if let Ok(f) = raw.parse::<f64>() {
        return serde_yaml_ng::Value::Number(serde_yaml_ng::Number::from(f));
    }
    // Try YAML array (e.g. "[a, b, c]")
    if raw.starts_with('[')
        && raw.ends_with(']')
        && let Ok(val) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(raw)
    {
        return val;
    }
    // Default to string
    serde_yaml_ng::Value::String(raw.to_string())
}

fn format_value(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::Null => "null".to_string(),
        // For complex values, use YAML serialization
        other => serde_yaml_ng::to_string(other).unwrap_or_else(|_| format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_yaml() -> serde_yaml_ng::Value {
        serde_yaml_ng::from_str(
            r#"
executor:
  default: claude
  mode: accept
spawn:
  max_parallel: 3
  auto_commit: true
packer:
  token_budget: 80000
  ignore_patterns:
    - "dist/**"
"#,
        )
        .unwrap()
    }

    #[test]
    fn resolve_top_level() {
        let root = sample_yaml();
        let val = resolve_path(&root, "executor").unwrap();
        assert!(val.is_mapping());
    }

    #[test]
    fn resolve_nested() {
        let root = sample_yaml();
        let val = resolve_path(&root, "executor.default").unwrap();
        assert_eq!(val.as_str().unwrap(), "claude");
    }

    #[test]
    fn resolve_number() {
        let root = sample_yaml();
        let val = resolve_path(&root, "spawn.max_parallel").unwrap();
        assert_eq!(val.as_u64().unwrap(), 3);
    }

    #[test]
    fn resolve_bool() {
        let root = sample_yaml();
        let val = resolve_path(&root, "spawn.auto_commit").unwrap();
        assert!(val.as_bool().unwrap());
    }

    #[test]
    fn resolve_missing() {
        let root = sample_yaml();
        assert!(resolve_path(&root, "nonexistent.key").is_none());
    }

    #[test]
    fn set_string_value() {
        let mut root = sample_yaml();
        set_path(
            &mut root,
            "executor.default",
            serde_yaml_ng::Value::String("opencode".into()),
        )
        .unwrap();
        assert_eq!(
            resolve_path(&root, "executor.default")
                .unwrap()
                .as_str()
                .unwrap(),
            "opencode"
        );
    }

    #[test]
    fn set_number_value() {
        let mut root = sample_yaml();
        set_path(
            &mut root,
            "spawn.max_parallel",
            serde_yaml_ng::Value::Number(5.into()),
        )
        .unwrap();
        assert_eq!(
            resolve_path(&root, "spawn.max_parallel")
                .unwrap()
                .as_u64()
                .unwrap(),
            5
        );
    }

    #[test]
    fn set_missing_key_errors() {
        let mut root = sample_yaml();
        let result = set_path(
            &mut root,
            "executor.nonexistent",
            serde_yaml_ng::Value::String("x".into()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn parse_value_bool() {
        assert_eq!(parse_value("true"), serde_yaml_ng::Value::Bool(true));
        assert_eq!(parse_value("false"), serde_yaml_ng::Value::Bool(false));
    }

    #[test]
    fn parse_value_integer() {
        assert_eq!(parse_value("42"), serde_yaml_ng::Value::Number(42.into()));
    }

    #[test]
    fn parse_value_string() {
        assert_eq!(
            parse_value("hello"),
            serde_yaml_ng::Value::String("hello".into())
        );
    }

    #[test]
    fn parse_value_array() {
        let val = parse_value("[a, b, c]");
        assert!(val.is_sequence());
    }

    #[test]
    fn format_value_string() {
        let val = serde_yaml_ng::Value::String("hello".into());
        assert_eq!(format_value(&val), "hello");
    }

    #[test]
    fn format_value_number() {
        let val = serde_yaml_ng::Value::Number(42.into());
        assert_eq!(format_value(&val), "42");
    }

    #[test]
    fn format_value_bool() {
        let val = serde_yaml_ng::Value::Bool(true);
        assert_eq!(format_value(&val), "true");
    }
}
