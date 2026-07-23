use std::collections::BTreeMap;
use std::fs;

use super::{decode_record, fixed_file, read_bounded, row_key};
use crate::InboxError;
use crate::limits::StoredLimits;
use crate::record::Record;
use amiss_controller::atomic_write_recovery::{
    ATOMIC_WRITE_DIRECTORY_PREFIX, AtomicWriteDirectory,
};

pub(crate) struct RootEntries {
    pub(crate) rows: BTreeMap<String, Row>,
    bytes: u64,
    temporary: Vec<AtomicWriteDirectory>,
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
        let record_reservation = limits.record_reservation().ok_or(InboxError::Corrupt)?;
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
                if !file_type.is_file()
                    || rows.contains_key(key)
                    || u64::try_from(rows.len()).unwrap_or(u64::MAX) >= limits.max_records()
                {
                    return Err(InboxError::Corrupt);
                }
                let metadata = entry.metadata()?;
                let row_bytes = metadata.len();
                if row_bytes > limits.max_record_bytes() || row_bytes > record_reservation {
                    return Err(InboxError::Corrupt);
                }
                bytes = bytes
                    .checked_add(row_bytes)
                    .filter(|total| *total <= limits.max_bytes())
                    .ok_or(InboxError::Corrupt)?;
                let encoded = read_bounded(&entry.path(), limits.max_record_bytes())?;
                if u64::try_from(encoded.len()).ok() != Some(row_bytes) {
                    return Err(InboxError::Corrupt);
                }
                let record = decode_record(&encoded)?;
                record.validate(key, limits)?;
                rows.insert(
                    key.to_owned(),
                    Row {
                        bytes: row_bytes,
                        record,
                    },
                );
                continue;
            }
            if name.starts_with(ATOMIC_WRITE_DIRECTORY_PREFIX) && file_type.is_dir() {
                if u64::try_from(temporary.len()).unwrap_or(u64::MAX) >= limits.max_records() {
                    return Err(InboxError::Corrupt);
                }
                temporary.push(AtomicWriteDirectory::read(entry.path())?);
                continue;
            }
            return Err(InboxError::Corrupt);
        }
        let row_count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
        if row_count > limits.max_records() || bytes > limits.max_bytes() {
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
