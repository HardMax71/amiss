use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::path::Path;

use crate::Error;

/// The handle boundary, in safe Rust on every supported platform. The root's
/// final entry is opened without following it, and every later entry is
/// opened relative to a held directory handle without following it: `openat`
/// with `O_NOFOLLOW` on unix, `NtCreateFile` with the directory handle as its
/// root and `FILE_FLAG_OPEN_REPARSE_POINT` on Windows. There is no pathname
/// traversal fallback anywhere below the root.
///
/// Unix refuses a symlink in the open itself. Windows opens the reparse point
/// rather than its target, so the refusal is the explicit attribute check in
/// [`ordinary`].
///
/// # Errors
///
/// `RepositoryUnavailable` when the root is absent, is not a directory, or is
/// a symlink, junction, or other reparse point.
pub(crate) fn open_root(root: &Path) -> Result<File, Error> {
    let file = root_options()
        .open(root)
        .map_err(|_defect| Error::RepositoryUnavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_defect| Error::RepositoryUnavailable)?;
    if !metadata.is_dir() || !ordinary(&metadata) {
        return Err(Error::RepositoryUnavailable);
    }
    Ok(file)
}

/// Opens one directory child relative to a held handle, refusing to follow it.
///
/// # Errors
///
/// The underlying open error, or `NotFound` shape when the entry exists but
/// is a reparse point or not a directory, so callers cannot mistake a
/// refused symlink for a readable directory.
pub(crate) fn open_dir(parent: &File, name: &str) -> io::Result<File> {
    let file = fs_at::OpenOptions::default()
        .read(true)
        .follow(false)
        .open_dir_at(parent, name)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || !ordinary(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not an ordinary directory",
        ));
    }
    Ok(file)
}

/// Opens one regular-file child relative to a held handle, refusing to follow
/// it.
///
/// # Errors
///
/// As [`open_dir`], for the regular-file case.
pub(crate) fn open_file(parent: &File, name: &str) -> io::Result<File> {
    let file = fs_at::OpenOptions::default()
        .read(true)
        .follow(false)
        .open_at(parent, name)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || !ordinary(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not an ordinary file",
        ));
    }
    Ok(file)
}

/// The names directly below a held directory handle, read from the handle
/// itself. Enumeration yields candidate names only; every one of them is
/// still opened through [`open_file`], so a name that no longer resolves to
/// an ordinary entry below this handle is refused rather than followed.
///
/// # Errors
///
/// The underlying enumeration error.
pub(crate) fn names(dir: &mut File) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in fs_at::read_dir(dir)? {
        let entry = entry?;
        if let Some(name) = entry.name().to_str()
            && name != "."
            && name != ".."
        {
            out.push(name.to_owned());
        }
    }
    Ok(out)
}

/// Whether an entry the platform already opened is an ordinary one. On unix
/// `O_NOFOLLOW` refused the symlink before this point. On Windows the entry
/// was opened as itself rather than followed, so a reparse point arrives here
/// and is refused by its attribute.
#[cfg(windows)]
fn ordinary(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes()
        & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
        == 0
}

#[cfg(unix)]
const fn ordinary(_metadata: &Metadata) -> bool {
    true
}

#[cfg(unix)]
fn root_options() -> OpenOptions {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC);
    options
}

/// Windows opens a directory handle only with backup semantics, and stops at
/// the reparse point rather than following it only with the reparse flag.
///
/// The access mask is spelled out rather than left to `read(true)`, which
/// would ask for `GENERIC_READ`. Every later open is an `NtCreateFile` whose
/// `RootDirectory` is this handle, and that requires `FILE_TRAVERSE`, which
/// `GENERIC_READ` does not carry. These are the rights `fs_at` itself takes
/// for the directory handles it returns, so the root behaves like every
/// directory below it.
#[cfg(windows)]
fn root_options() -> OpenOptions {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_TRAVERSE, SYNCHRONIZE,
    };
    let mut options = OpenOptions::new();
    options
        .access_mode(SYNCHRONIZE | FILE_READ_ATTRIBUTES | FILE_LIST_DIRECTORY | FILE_TRAVERSE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options
}

/// One positioned read of exactly `buf.len()` bytes, which pack access needs
/// and which neither platform's `Read` gives without moving a shared cursor.
///
/// # Errors
///
/// A short read at end of file, or the underlying read error.
#[cfg(unix)]
pub(crate) fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt as _;
    file.read_exact_at(buf, offset)
}

#[cfg(windows)]
pub(crate) fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt as _;
    let mut written = 0_usize;
    while written < buf.len() {
        let at = offset
            .checked_add(u64::try_from(written).unwrap_or(u64::MAX))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "offset overflow"))?;
        let slice = buf
            .get_mut(written..)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "buffer overflow"))?;
        match file.seek_read(slice, at) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "short positioned read",
                ));
            }
            Ok(count) => written = written.saturating_add(count),
            Err(defect) if defect.kind() == io::ErrorKind::Interrupted => {}
            Err(defect) => return Err(defect),
        }
    }
    Ok(())
}
