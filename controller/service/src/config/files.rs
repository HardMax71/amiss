use std::io::Read as _;
use std::path::Path;

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _, OpenOptionsSyncExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
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
        .map_err(|_defect| ConfigError("configuration is not strict JSON"))
}

/// Reads one absolute, bounded, non-symlink regular file.
///
/// # Errors
///
/// The path is relative, inaccessible, not a regular file, or exceeds the
/// supplied byte limit.
pub fn read_regular(path: &Path, maximum: u64) -> Result<Vec<u8>, ConfigError> {
    if !path.is_absolute() {
        return Err(ConfigError("trust files must use absolute paths"));
    }
    let parent = path
        .parent()
        .ok_or(ConfigError("a trust file cannot be read"))?;
    let name = path
        .file_name()
        .ok_or(ConfigError("a trust file cannot be read"))?;
    let directory = Dir::open_ambient_dir(parent, ambient_authority())
        .map_err(|_defect| ConfigError("a trust file cannot be read"))?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No).nonblock(true);
    let file = directory
        .open_with(name, &options)
        .map_err(|_defect| ConfigError("a trust file cannot be read"))?;
    let metadata = file
        .metadata()
        .map_err(|_defect| ConfigError("a trust file cannot be read"))?;
    if !metadata.file_type().is_file() || metadata.len() > maximum {
        return Err(ConfigError("a trust file is not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_defect| ConfigError("a trust file cannot be read"))?;
    let length =
        u64::try_from(bytes.len()).map_err(|_defect| ConfigError("a trust file is too large"))?;
    (length <= maximum)
        .then_some(bytes)
        .ok_or(ConfigError("a trust file is too large"))
}
