use std::path::Path;

use super::ConfigError;

pub const CONFIG_BYTES: u64 = 65_536;

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
