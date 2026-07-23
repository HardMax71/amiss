use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::PathBuf;

pub const ATOMIC_WRITE_DIRECTORY_PREFIX: &str = ".atomicwrite";

#[derive(Debug)]
pub enum AtomicWriteDirectoryError {
    Io(io::Error),
    Malformed,
}

pub struct AtomicWriteDirectory {
    path: PathBuf,
    file: Option<PathBuf>,
}

impl AtomicWriteDirectory {
    /// Validates the only temporary directory shape emitted by `atomicwrites`.
    ///
    /// # Errors
    ///
    /// Returns `Malformed` for unexpected contents and `Io` when inspection
    /// fails.
    pub fn read(path: PathBuf) -> Result<Self, AtomicWriteDirectoryError> {
        let mut file = None;
        for entry in fs::read_dir(&path).map_err(AtomicWriteDirectoryError::Io)? {
            let entry = entry.map_err(AtomicWriteDirectoryError::Io)?;
            if file.is_some() || entry.file_name() != OsStr::new("tmpfile.tmp") {
                return Err(AtomicWriteDirectoryError::Malformed);
            }
            if !entry
                .file_type()
                .map_err(AtomicWriteDirectoryError::Io)?
                .is_file()
            {
                return Err(AtomicWriteDirectoryError::Malformed);
            }
            file = Some(entry.path());
        }
        Ok(Self { path, file })
    }

    /// Removes a validated interrupted-write directory.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when either the temporary file or directory cannot
    /// be removed.
    pub fn remove(self) -> Result<(), io::Error> {
        if let Some(file) = self.file {
            fs::remove_file(file)?;
        }
        fs::remove_dir(self.path)
    }
}
