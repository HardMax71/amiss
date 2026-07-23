mod cleanup;
mod metadata;
mod report;

use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use atomicwrites::{AllowOverwrite, AtomicFile};

use self::metadata::RootMetadata;
use super::format::{self, Record};
use super::{FileLedgerConfig, FileLedgerError};
use crate::atomic_write_recovery::{ATOMIC_WRITE_DIRECTORY_PREFIX, AtomicWriteDirectory};
use crate::ingress::ReplayKeep;

const MAINTENANCE_LOCK: &str = ".amiss-maintenance.lock";
const ADMISSION_LOCK: &str = ".amiss-admission.lock";
const CLOCK_LOCK: &str = ".amiss-clock.lock";
const METADATA_FILE: &str = ".amiss-root.state";

pub(super) struct Store {
    root: PathBuf,
    config: FileLedgerConfig,
}

impl Store {
    pub(super) fn open(
        root: &Path,
        config: FileLedgerConfig,
        now: i64,
    ) -> Result<Self, FileLedgerError> {
        if !fs::symlink_metadata(root)?.file_type().is_dir() || now < 0 {
            return Err(FileLedgerError::Corrupt);
        }
        let root = fs::canonicalize(root)?;
        let maintenance = open_lock(&root.join(MAINTENANCE_LOCK))?;
        maintenance.lock()?;
        let metadata = load_or_create_metadata(&root, config, now)?;
        if !metadata.matches(config) {
            return Err(FileLedgerError::Configuration);
        }
        Ok(Self { root, config })
    }

    pub(super) fn lock(&self, key: &str, replay_keep: ReplayKeep) -> Result<Row, FileLedgerError> {
        validate_key(key)?;
        if let ReplayKeep::KeepThrough { window, .. } = replay_keep
            && window != self.config.replay_window()
        {
            return Err(FileLedgerError::Configuration);
        }
        let maintenance = open_lock(&self.root.join(MAINTENANCE_LOCK))?;
        maintenance.lock_shared()?;
        let row_lock = open_lock(&self.root.join(row_lock_name(key)?))?;
        row_lock.lock()?;
        Ok(Row {
            root: self.root.clone(),
            key: key.to_owned(),
            config: self.config,
            _row_lock: row_lock,
            _maintenance: maintenance,
        })
    }
}

pub(super) struct Row {
    pub(super) root: PathBuf,
    pub(super) key: String,
    config: FileLedgerConfig,
    _row_lock: File,
    _maintenance: File,
}

impl Row {
    pub(super) fn load(&self) -> Result<Option<Record>, FileLedgerError> {
        let path = self.state_path();
        match read_bounded(&path, format::MAX_RECORD_BYTES) {
            Ok(bytes) => {
                let record = format::decode(&bytes)?;
                if !record.matches_key(&self.key)? {
                    return Err(FileLedgerError::Corrupt);
                }
                Ok(Some(record))
            }
            Err(FileLedgerError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn save_new(&self, record: &Record) -> Result<(), FileLedgerError> {
        let admission = open_lock(&self.root.join(ADMISSION_LOCK))?;
        admission.lock()?;
        if self.load()?.is_some() {
            return Err(FileLedgerError::Corrupt);
        }
        if count_records(&self.root, self.config.max_records())? >= self.config.max_records() {
            return Err(FileLedgerError::Full);
        }
        self.save(record)
    }

    pub(super) fn observe_clock(&self, now: i64) -> Result<i64, FileLedgerError> {
        if now < 0 {
            return Err(FileLedgerError::Clock);
        }
        let clock = open_lock(&self.root.join(CLOCK_LOCK))?;
        clock.lock()?;
        let mut metadata = load_metadata(&self.root)?;
        if !metadata.matches(self.config) {
            return Err(FileLedgerError::Configuration);
        }
        let previous = metadata.clock_high_water_unix_millis();
        let effective = metadata.advance_clock(now)?;
        if effective != previous {
            atomic_write(
                &self.root.join(METADATA_FILE),
                &metadata::encode(&metadata)?,
            )?;
        }
        Ok(effective)
    }

    pub(super) fn save(&self, record: &Record) -> Result<(), FileLedgerError> {
        atomic_write(&self.state_path(), &format::encode(record)?)
    }

    fn state_path(&self) -> PathBuf {
        self.root.join(format!("{}.state", self.key))
    }
}

fn load_or_create_metadata(
    root: &Path,
    config: FileLedgerConfig,
    now: i64,
) -> Result<RootMetadata, FileLedgerError> {
    let path = root.join(METADATA_FILE);
    match read_bounded(&path, metadata::MAX_METADATA_BYTES) {
        Ok(bytes) => metadata::decode(&bytes),
        Err(FileLedgerError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            prepare_new_root(root)?;
            let metadata = RootMetadata::new(config, now);
            atomic_write(&path, &metadata::encode(&metadata)?)?;
            Ok(metadata)
        }
        Err(error) => Err(error),
    }
}

fn prepare_new_root(root: &Path) -> Result<(), FileLedgerError> {
    let mut temporary = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(FileLedgerError::Corrupt)?;
        let file_type = entry.file_type()?;
        if name == MAINTENANCE_LOCK && file_type.is_file() {
            continue;
        }
        if name.starts_with(ATOMIC_WRITE_DIRECTORY_PREFIX) && file_type.is_dir() {
            temporary.push(AtomicWriteDirectory::read(entry.path())?);
            continue;
        }
        return Err(FileLedgerError::Corrupt);
    }
    for directory in temporary {
        directory.remove()?;
    }
    Ok(())
}

fn load_metadata(root: &Path) -> Result<RootMetadata, FileLedgerError> {
    let bytes = read_bounded(&root.join(METADATA_FILE), metadata::MAX_METADATA_BYTES)?;
    metadata::decode(&bytes)
}

fn count_records(root: &Path, maximum: u64) -> Result<u64, FileLedgerError> {
    let mut count = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !is_state_name(&entry.file_name()) {
            continue;
        }
        if !entry.file_type()?.is_file() {
            return Err(FileLedgerError::Corrupt);
        }
        count = count.checked_add(1).ok_or(FileLedgerError::Corrupt)?;
        if count >= maximum {
            return Ok(count);
        }
    }
    Ok(count)
}

fn is_state_name(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name.strip_suffix(".state")
        .is_some_and(|key| validate_key(key).is_ok())
}

fn row_lock_name(key: &str) -> Result<String, FileLedgerError> {
    let mut bytes = key.bytes();
    let high = hex_value(bytes.next().ok_or(FileLedgerError::Corrupt)?)?;
    let low = hex_value(bytes.next().ok_or(FileLedgerError::Corrupt)?)?;
    let shard = high
        .checked_mul(16)
        .and_then(|value| value.checked_add(low))
        .ok_or(FileLedgerError::Corrupt)?;
    Ok(format!(".amiss-row-{shard:02x}.lock"))
}

fn hex_value(byte: u8) -> Result<u8, FileLedgerError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(FileLedgerError::Corrupt),
    }
}

fn validate_key(key: &str) -> Result<(), FileLedgerError> {
    if key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(FileLedgerError::Corrupt)
    }
}

fn open_lock(path: &Path) -> Result<File, FileLedgerError> {
    reject_non_file(path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if !lock.metadata()?.is_file() {
        return Err(FileLedgerError::Corrupt);
    }
    Ok(lock)
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), FileLedgerError> {
    reject_non_file(path)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(io::Error::from)?;
    Ok(())
}

pub(super) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, FileLedgerError> {
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
