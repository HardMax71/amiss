#![forbid(unsafe_code)]

use std::io::Read as _;
use std::path::Path;

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _, OpenOptionsSyncExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};

/// Reads one absolute bounded regular file without following its final entry.
///
/// # Errors
///
/// The path is relative, inaccessible, not a regular file, or exceeds the
/// supplied byte limit.
pub fn read_bounded(path: &Path, maximum: u64) -> std::io::Result<Vec<u8>> {
    if !path.is_absolute() {
        return Err(std::io::Error::other("path is not absolute"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("file has no parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("file has no name"))?;
    let directory = Dir::open_ambient_dir(parent, ambient_authority())?;
    read_bounded_at(&directory, Path::new(name), maximum)
}

/// Reads one relative bounded regular file beneath an open directory without
/// following its final entry.
///
/// # Errors
///
/// The path is absolute, inaccessible, not a regular file, or exceeds the
/// supplied byte limit.
pub fn read_bounded_at(directory: &Dir, path: &Path, maximum: u64) -> std::io::Result<Vec<u8>> {
    if path.is_absolute() {
        return Err(std::io::Error::other("path is not relative"));
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No).nonblock(true);
    let file = directory.open_with(path, &options)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > maximum {
        return Err(std::io::Error::other("not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > maximum) {
        return Err(std::io::Error::other("file grew past its bound"));
    }
    Ok(bytes)
}
