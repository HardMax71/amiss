mod entries;

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use amiss_controller::atomic_write_recovery::{
    ATOMIC_WRITE_DIRECTORY_PREFIX, AtomicWriteDirectory,
};
use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};

pub(crate) use self::entries::RootEntries;
use crate::InboxError;
use crate::frame;
use crate::limits::StoredLimits;
use crate::record::Record;

const LOCK_FILE: &str = ".amiss-inbox.lock";
const METADATA_FILE: &str = ".amiss-inbox.state";
const METADATA_SCHEMA: &str = "amiss/controller-inbox-root-v1";
const METADATA_MAGIC: &[u8] = b"AMISS-INBOX-ROOT";
const METADATA_DOMAIN: &str = "amiss/controller-inbox-root-frame-v1";
const RECORD_MAGIC: &[u8] = b"AMISS-INBOX-ROW";
const RECORD_DOMAIN: &str = "amiss/controller-inbox-row-frame-v1";
const MAX_METADATA_BYTES: u64 = 4_096;

pub(crate) struct Store {
    root: PathBuf,
    limits: StoredLimits,
    _owner_lock: File,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RootMetadata {
    schema: String,
    limits: StoredLimits,
}

impl Store {
    pub(crate) fn open(root: &Path, limits: StoredLimits) -> Result<Self, InboxError> {
        if !fs::symlink_metadata(root)?.file_type().is_dir() {
            return Err(InboxError::Corrupt);
        }
        let root = fs::canonicalize(root)?;
        if !fs::symlink_metadata(&root)?.file_type().is_dir() {
            return Err(InboxError::Corrupt);
        }
        let owner_lock = open_lock(&root.join(LOCK_FILE))?;
        match owner_lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(InboxError::AlreadyOpen),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        load_or_create_metadata(&root, limits)?;
        let store = Self {
            root,
            limits,
            _owner_lock: owner_lock,
        };
        store.scan()?;
        Ok(store)
    }

    pub(crate) fn scan(&self) -> Result<RootEntries, InboxError> {
        let mut entries = RootEntries::read(&self.root, self.limits)?;
        entries.remove_temporary()?;
        Ok(entries)
    }

    pub(crate) fn save_new(
        &self,
        entries: &RootEntries,
        key: &str,
        record: &Record,
    ) -> Result<(), InboxError> {
        if entries.count() >= self.limits.max_records() {
            return Err(InboxError::Full);
        }
        let rows_after = entries.count().checked_add(1).ok_or(InboxError::Full)?;
        self.reserve_transition(rows_after)?;
        self.save(entries, key, record, 0)
    }

    pub(crate) fn remove(&self, key: &str) -> Result<(), InboxError> {
        validate_key(key)?;
        let path = self.root.join(row_name(key));
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err(InboxError::Corrupt);
        }
        fs::remove_file(path)?;
        Ok(())
    }

    pub(crate) fn save(
        &self,
        entries: &RootEntries,
        key: &str,
        record: &Record,
        old_bytes: u64,
    ) -> Result<(), InboxError> {
        validate_key(key)?;
        let bytes = encode_record(record)?;
        let encoded_bytes = u64::try_from(bytes.len()).map_err(|_| InboxError::Full)?;
        let record_reservation = self
            .limits
            .record_reservation()
            .ok_or(InboxError::Corrupt)?;
        if encoded_bytes > record_reservation || encoded_bytes > self.limits.max_record_bytes() {
            return Err(InboxError::Full);
        }
        let _durable_after = entries
            .bytes()
            .checked_sub(old_bytes)
            .and_then(|bytes| bytes.checked_add(encoded_bytes))
            .ok_or(InboxError::Corrupt)?;
        let actual_with_atomic_copy = entries
            .bytes()
            .checked_add(encoded_bytes)
            .ok_or(InboxError::Full)?;
        if actual_with_atomic_copy > self.limits.max_bytes() {
            return Err(InboxError::Full);
        }
        atomic_write(&self.root.join(row_name(key)), &bytes)
    }

    fn reserve_transition(&self, rows_after: u64) -> Result<(), InboxError> {
        rows_after
            .checked_add(1)
            .and_then(|rows| rows.checked_mul(self.limits.record_reservation()?))
            .filter(|bytes| *bytes <= self.limits.max_bytes())
            .map(|_bytes| ())
            .ok_or(InboxError::Full)
    }
}

pub(crate) fn encode_record(record: &Record) -> Result<Vec<u8>, InboxError> {
    frame::encode(RECORD_MAGIC, RECORD_DOMAIN, record)
}

pub(crate) fn decode_record(bytes: &[u8]) -> Result<Record, InboxError> {
    frame::decode(RECORD_MAGIC, RECORD_DOMAIN, bytes)
}

pub(crate) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, InboxError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > maximum {
        return Err(InboxError::Corrupt);
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(InboxError::Corrupt);
    }
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(InboxError::Corrupt);
    }
    Ok(bytes)
}

fn load_or_create_metadata(root: &Path, limits: StoredLimits) -> Result<(), InboxError> {
    let path = root.join(METADATA_FILE);
    let metadata = match read_bounded(&path, MAX_METADATA_BYTES) {
        Ok(bytes) => frame::decode(METADATA_MAGIC, METADATA_DOMAIN, &bytes)?,
        Err(InboxError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            prepare_new_root(root)?;
            let metadata = RootMetadata {
                schema: METADATA_SCHEMA.to_owned(),
                limits,
            };
            atomic_write(
                &path,
                &frame::encode(METADATA_MAGIC, METADATA_DOMAIN, &metadata)?,
            )?;
            metadata
        }
        Err(error) => return Err(error),
    };
    if metadata.schema != METADATA_SCHEMA || metadata.limits != limits {
        return Err(InboxError::Configuration);
    }
    Ok(())
}

fn prepare_new_root(root: &Path) -> Result<(), InboxError> {
    let mut temporary = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(InboxError::Corrupt)?;
        let file_type = entry.file_type()?;
        if name == LOCK_FILE && file_type.is_file() {
            continue;
        }
        if name.starts_with(ATOMIC_WRITE_DIRECTORY_PREFIX) && file_type.is_dir() {
            temporary.push(AtomicWriteDirectory::read(entry.path())?);
            continue;
        }
        return Err(InboxError::Corrupt);
    }
    for directory in temporary {
        directory.remove()?;
    }
    Ok(())
}

fn open_lock(path: &Path) -> Result<File, InboxError> {
    reject_non_file(path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if !lock.metadata()?.is_file() {
        return Err(InboxError::Corrupt);
    }
    Ok(lock)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), InboxError> {
    reject_non_file(path)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(io::Error::from)?;
    Ok(())
}

fn reject_non_file(path: &Path) -> Result<(), InboxError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(InboxError::Corrupt),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn row_name(key: &str) -> String {
    format!("{key}.row")
}

pub(crate) fn row_key(name: &str) -> Option<&str> {
    let key = name.strip_suffix(".row")?;
    validate_key(key).ok()?;
    Some(key)
}

fn validate_key(key: &str) -> Result<(), InboxError> {
    if crate::hash::is_digest(key) {
        Ok(())
    } else {
        Err(InboxError::Corrupt)
    }
}

pub(crate) fn fixed_file(name: &str) -> bool {
    matches!(name, LOCK_FILE | METADATA_FILE)
}
