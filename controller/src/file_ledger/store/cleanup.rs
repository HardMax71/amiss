use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use amiss_wire::report::MACHINE_JSON_BYTES;

use super::{
    MAINTENANCE_LOCK, METADATA_FILE, Store, atomic_write, is_state_name, load_metadata, metadata,
    open_lock, read_bounded, validate_key,
};
use crate::atomic_write_recovery::{ATOMIC_WRITE_DIRECTORY_PREFIX, AtomicWriteDirectory};
use crate::file_ledger::format::{self, Record, State};
use crate::file_ledger::{FileLedgerCleanup, FileLedgerError};

impl Store {
    pub(in crate::file_ledger) fn cleanup(
        &self,
        now: i64,
    ) -> Result<FileLedgerCleanup, FileLedgerError> {
        let maintenance = open_lock(&self.root.join(MAINTENANCE_LOCK))?;
        maintenance.lock()?;
        let mut metadata = load_metadata(&self.root)?;
        if !metadata.matches(self.config) {
            return Err(FileLedgerError::Configuration);
        }
        let previous = metadata.clock_high_water_unix_millis();
        let effective_now = metadata.advance_clock(now)?;
        if effective_now != previous {
            atomic_write(
                &self.root.join(METADATA_FILE),
                &metadata::encode(&metadata)?,
            )?;
        }
        let mut root = RootEntries::read(&self.root, self.config.max_records())?;
        root.validate_reports()?;
        root.remove(effective_now)
    }
}

struct RootEntries {
    states: BTreeMap<String, (PathBuf, Record)>,
    reports: BTreeMap<String, PathBuf>,
    temporary: Vec<AtomicWriteDirectory>,
}

impl RootEntries {
    fn read(root: &Path, maximum_records: u64) -> Result<Self, FileLedgerError> {
        let mut entries = Self {
            states: BTreeMap::new(),
            reports: BTreeMap::new(),
            temporary: Vec::new(),
        };
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(FileLedgerError::Corrupt)?;
            let file_type = entry.file_type()?;
            if is_fixed_file(name) || is_row_lock(name) {
                if !file_type.is_file() {
                    return Err(FileLedgerError::Corrupt);
                }
            } else if let Some(key) = state_key(name) {
                if !file_type.is_file() || entries.states.contains_key(key) {
                    return Err(FileLedgerError::Corrupt);
                }
                let bytes = read_bounded(&entry.path(), format::MAX_RECORD_BYTES)?;
                let record = format::decode(&bytes)?;
                if !record.matches_key(key)? {
                    return Err(FileLedgerError::Corrupt);
                }
                entries
                    .states
                    .insert(key.to_owned(), (entry.path(), record));
            } else if let Some(key) = report_key(name) {
                if !file_type.is_file()
                    || entries
                        .reports
                        .insert(key.to_owned(), entry.path())
                        .is_some()
                {
                    return Err(FileLedgerError::Corrupt);
                }
            } else if name.starts_with(ATOMIC_WRITE_DIRECTORY_PREFIX) {
                if !file_type.is_dir() {
                    return Err(FileLedgerError::Corrupt);
                }
                entries
                    .temporary
                    .push(AtomicWriteDirectory::read(entry.path())?);
            } else {
                return Err(FileLedgerError::Corrupt);
            }
        }
        if u64::try_from(entries.states.len()).unwrap_or(u64::MAX) > maximum_records {
            return Err(FileLedgerError::Corrupt);
        }
        Ok(entries)
    }

    fn validate_reports(&self) -> Result<(), FileLedgerError> {
        for (key, (_, record)) in &self.states {
            let State::Staged { publication, .. } = &record.state else {
                continue;
            };
            match (publication.report(), self.reports.get(key)) {
                (None, _) => {}
                (Some(_), None) => return Err(FileLedgerError::Corrupt),
                (Some(reference), Some(path)) => {
                    let bytes = read_bounded(path, MACHINE_JSON_BYTES)?;
                    if !reference.matches(&bytes) {
                        return Err(FileLedgerError::Corrupt);
                    }
                }
            }
        }
        Ok(())
    }

    fn remove(&mut self, now: i64) -> Result<FileLedgerCleanup, FileLedgerError> {
        let report_keys = self.reports.keys().cloned().collect::<Vec<_>>();
        let mut removed_reports = 0_u64;
        for key in report_keys {
            if self.report_is_live(&key) {
                continue;
            }
            let path = self.reports.remove(&key).ok_or(FileLedgerError::Corrupt)?;
            fs::remove_file(path)?;
            removed_reports = removed_reports
                .checked_add(1)
                .ok_or(FileLedgerError::Corrupt)?;
        }

        let state_keys = self
            .states
            .iter()
            .filter(|&(_, (_, record))| record.is_done_and_expired(now))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut removed_records = 0_u64;
        for key in state_keys {
            let (path, _) = self.states.remove(&key).ok_or(FileLedgerError::Corrupt)?;
            fs::remove_file(path)?;
            removed_records = removed_records
                .checked_add(1)
                .ok_or(FileLedgerError::Corrupt)?;
        }

        let mut removed_temporary = 0_u64;
        for directory in self.temporary.drain(..) {
            directory.remove()?;
            removed_temporary = removed_temporary
                .checked_add(1)
                .ok_or(FileLedgerError::Corrupt)?;
        }
        Ok(FileLedgerCleanup {
            removed_records,
            removed_reports,
            removed_temporary,
        })
    }

    fn report_is_live(&self, key: &str) -> bool {
        self.states.get(key).is_some_and(|(_, record)| {
            matches!(
                &record.state,
                State::Staged { publication, .. } if publication.report().is_some()
            )
        })
    }
}

fn is_fixed_file(name: &str) -> bool {
    matches!(
        name,
        MAINTENANCE_LOCK | super::ADMISSION_LOCK | super::CLOCK_LOCK | METADATA_FILE
    )
}

fn is_row_lock(name: &str) -> bool {
    let Some(shard) = name
        .strip_prefix(".amiss-row-")
        .and_then(|name| name.strip_suffix(".lock"))
    else {
        return false;
    };
    shard.len() == 2
        && shard
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn state_key(name: &str) -> Option<&str> {
    let key = name.strip_suffix(".state")?;
    (is_state_name(OsStr::new(name)) && validate_key(key).is_ok()).then_some(key)
}

fn report_key(name: &str) -> Option<&str> {
    let key = name.strip_suffix(".report")?;
    validate_key(key).is_ok().then_some(key)
}
