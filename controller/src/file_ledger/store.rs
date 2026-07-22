use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use atomicwrites::{AllowOverwrite, AtomicFile};

use super::FileLedgerError;
use super::format::{self, Record};

pub(super) struct Store {
    root: PathBuf,
}

impl Store {
    pub(super) fn open(root: &Path) -> Result<Self, FileLedgerError> {
        if !fs::symlink_metadata(root)?.file_type().is_dir() {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(Self {
            root: fs::canonicalize(root)?,
        })
    }

    pub(super) fn lock(&self, key: &str) -> Result<Row, FileLedgerError> {
        if key.len() != 64
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(FileLedgerError::Corrupt);
        }
        let lock_path = self.root.join(format!("{key}.lock"));
        reject_non_file(&lock_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        if !lock.metadata()?.is_file() {
            return Err(FileLedgerError::Corrupt);
        }
        lock.lock()?;
        Ok(Row {
            root: self.root.clone(),
            key: key.to_owned(),
            _lock: lock,
        })
    }
}

pub(super) struct Row {
    root: PathBuf,
    key: String,
    _lock: File,
}

impl Row {
    pub(super) fn load(&self) -> Result<Option<Record>, FileLedgerError> {
        let path = self.state_path();
        match read_bounded(&path, format::MAX_RECORD_BYTES) {
            Ok(bytes) => format::decode(&bytes).map(Some),
            Err(FileLedgerError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn save(&self, record: &Record) -> Result<(), FileLedgerError> {
        atomic_write(&self.state_path(), &format::encode(record)?)
    }

    fn state_path(&self) -> PathBuf {
        self.root.join(format!("{}.state", self.key))
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), FileLedgerError> {
    reject_non_file(path)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(io::Error::from)?;
    Ok(())
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, FileLedgerError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > maximum {
        return Err(FileLedgerError::Corrupt);
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(FileLedgerError::Corrupt);
    }
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(bytes)
}

fn reject_non_file(path: &Path) -> Result<(), FileLedgerError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(FileLedgerError::Corrupt),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
mod report;
