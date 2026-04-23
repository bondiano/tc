use std::path::Path;

use tc_core::config::TcConfig;

use crate::error::{StorageError, StorageResult};

pub fn load(path: &Path) -> StorageResult<TcConfig> {
    let content = std::fs::read_to_string(path).map_err(|e| StorageError::file_read(path, e))?;
    let config: TcConfig =
        serde_yaml_ng::from_str(&content).map_err(|e| StorageError::yaml_parse(path, e))?;
    config.validate().map_err(StorageError::ConfigValidation)?;
    Ok(config)
}

pub fn save(path: &Path, config: &TcConfig) -> StorageResult<()> {
    let content = serde_yaml_ng::to_string(config).map_err(StorageError::YamlSerialize)?;
    std::fs::write(path, content).map_err(|e| StorageError::file_write(path, e))?;
    Ok(())
}
