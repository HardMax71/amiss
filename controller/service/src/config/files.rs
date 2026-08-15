use std::path::Path;

use serde::de::DeserializeOwned;

use super::ConfigError;

const CONFIG_BYTES: u64 = 65_536;

/// Loads one bounded regular file as strict JSON.
///
/// # Errors
///
/// The path is not an absolute bounded regular file or its contents do not
/// satisfy the target's serde contract.
pub fn read_strict_json<T: DeserializeOwned>(path: &Path) -> Result<T, ConfigError> {
    serde_json::from_slice(&read_regular(path, CONFIG_BYTES)?)
        .map_err(|defect| ConfigError::caused_by("configuration is not strict JSON", defect))
}

/// Reads one absolute, bounded, non-symlink regular file.
///
/// # Errors
///
/// The path is relative, inaccessible, not a regular file, or exceeds the
/// supplied byte limit.
pub fn read_regular(path: &Path, maximum: u64) -> Result<Vec<u8>, ConfigError> {
    amiss_controller_files::read_bounded(path, maximum)
        .map_err(|defect| ConfigError::caused_by("a trust file cannot be read", defect))
}
