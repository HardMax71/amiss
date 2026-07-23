use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

use super::{decode_record, fixed_file, read_bounded, row_key};
use crate::InboxError;
use crate::limits::StoredLimits;
use crate::record::Record;

pub(crate) const ATOMIC_DIRECTORY_PREFIX: &str = ".atomicwrite";

pub(crate) struct RootEntries {
    pub(crate) rows: BTreeMap<String, Row>,
    bytes: u64,
    temporary: Vec<TemporaryDirectory>,
}

pub(crate) struct Row {
    pub(crate) bytes: u64,
    pub(crate) record: Record,
}

impl RootEntries {
    pub(crate) fn read(root: &std::path::Path, limits: StoredLimits) -> Result<Self, InboxError> {
        let mut rows = BTreeMap::new();
        let mut bytes = 0_u64;
        let mut temporary = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(InboxError::Corrupt)?;
            let file_type = entry.file_type()?;
            if fixed_file(name) {
                if !file_type.is_file() {
                    return Err(InboxError::Corrupt);
                }
                continue;
            }
            if let Some(key) = row_key(name) {
                if !file_type.is_file() || rows.contains_key(key) {
                    return Err(InboxError::Corrupt);
                }
                let metadata = entry.metadata()?;
                let row_bytes = metadata.len();
                if row_bytes > limits.max_record_bytes() {
                    return Err(InboxError::Corrupt);
                }
                let encoded = read_bounded(&entry.path(), limits.max_record_bytes())?;
                if u64::try_from(encoded.len()).ok() != Some(row_bytes) {
                    return Err(InboxError::Corrupt);
                }
                let record = decode_record(&encoded)?;
                record.validate(key, limits)?;
                bytes = bytes.checked_add(row_bytes).ok_or(InboxError::Corrupt)?;
                rows.insert(
                    key.to_owned(),
                    Row {
                        bytes: row_bytes,
                        record,
                    },
                );
                continue;
            }
            if name.starts_with(ATOMIC_DIRECTORY_PREFIX) && file_type.is_dir() {
                temporary.push(TemporaryDirectory::read(entry.path())?);
                continue;
            }
            return Err(InboxError::Corrupt);
        }
        if u64::try_from(rows.len()).unwrap_or(u64::MAX) > limits.max_records()
            || bytes > limits.max_bytes()
        {
            return Err(InboxError::Corrupt);
        }
        Ok(Self {
            rows,
            bytes,
            temporary,
        })
    }

    pub(crate) fn count(&self) -> u64 {
        u64::try_from(self.rows.len()).unwrap_or(u64::MAX)
    }

    pub(crate) const fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn remove_temporary(&mut self) -> Result<(), InboxError> {
        for directory in self.temporary.drain(..) {
            directory.remove()?;
        }
        Ok(())
    }
}

pub(crate) struct TemporaryDirectory {
    path: PathBuf,
    file: Option<PathBuf>,
}

impl TemporaryDirectory {
    pub(crate) fn read(path: PathBuf) -> Result<Self, InboxError> {
        let mut file = None;
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            if file.is_some() || entry.file_name() != OsStr::new("tmpfile.tmp") {
                return Err(InboxError::Corrupt);
            }
            if !entry.file_type()?.is_file() {
                return Err(InboxError::Corrupt);
            }
            file = Some(entry.path());
        }
        Ok(Self { path, file })
    }

    pub(crate) fn remove(self) -> Result<(), InboxError> {
        if let Some(file) = self.file {
            fs::remove_file(file)?;
        }
        fs::remove_dir(self.path)?;
        Ok(())
    }
}
