use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use atomicwrites::{AllowOverwrite, AtomicFile, DisallowOverwrite};

use super::format::{self, Blob, Record, RecordInput, Root};
use super::{
    ArtifactBundle, ArtifactCleanup, ArtifactComponent, ArtifactError, ArtifactReference,
    ArtifactStoreConfig,
};
use crate::atomic_write_recovery::{ATOMIC_WRITE_DIRECTORY_PREFIX, AtomicWriteDirectory};
use crate::{ControllerClock, ControllerEvaluationId};

const OWNER_LOCK: &str = ".amiss-artifacts.lock";
const ROOT_STATE: &str = ".amiss-artifacts.state";

pub struct FileArtifactStore {
    root: PathBuf,
    config: ArtifactStoreConfig,
    clock: Arc<dyn ControllerClock>,
    state: Mutex<State>,
    _owner_lock: File,
}

struct State {
    root: Root,
    records: BTreeMap<String, StoredRecord>,
    evaluations: BTreeMap<String, String>,
    bytes: u64,
    trusted: bool,
}

struct StoredRecord {
    metadata: Record,
    bytes: u64,
}

impl FileArtifactStore {
    /// Opens one exclusively owned artifact root and validates every retained byte.
    ///
    /// # Errors
    ///
    /// The limits, clock, root layout, retained metadata, or payload bytes are invalid.
    pub fn open_with_clock(
        root: &Path,
        config: ArtifactStoreConfig,
        clock: Arc<dyn ControllerClock>,
    ) -> Result<Self, ArtifactError> {
        validate_config(&config)?;
        if !fs::symlink_metadata(root)?.file_type().is_dir() {
            return Err(ArtifactError::Corrupt);
        }
        let root = fs::canonicalize(root)?;
        if !fs::symlink_metadata(&root)?.file_type().is_dir() {
            return Err(ArtifactError::Corrupt);
        }
        let owner_lock = open_lock(&root.join(OWNER_LOCK))?;
        match owner_lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(ArtifactError::AlreadyOpen),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        let now = trusted_now(clock.as_ref())?;
        let mut root_state = load_or_create_root(&root, &config, now)?;
        if root_state.schema != format::ROOT_SCHEMA
            || root_state.retention_millis != format::millis(config.retention)?
            || root_state.base_url != config.base_url
            || root_state.max_records != config.max_records
            || root_state.max_bytes != config.max_bytes
            || root_state.max_record_bytes != config.max_record_bytes
            || root_state.clock_high_water_unix_millis < 0
        {
            return Err(ArtifactError::Configuration);
        }
        let effective_now = now.max(root_state.clock_high_water_unix_millis);
        if effective_now != root_state.clock_high_water_unix_millis {
            root_state.clock_high_water_unix_millis = effective_now;
            save_root(&root, &root_state)?;
        }
        let state = scan(&root, &config, root_state)?;
        let store = Self {
            root,
            config,
            clock,
            state: Mutex::new(state),
            _owner_lock: owner_lock,
        };
        store.cleanup()?;
        Ok(store)
    }

    /// Retains exact bytes once for an evaluation and returns its stable locator.
    ///
    /// # Errors
    ///
    /// The bundle is invalid, the evaluation was rebound, capacity is exhausted,
    /// or durable storage cannot be trusted.
    pub fn retain(
        &self,
        evaluation_id: &ControllerEvaluationId,
        bundle: ArtifactBundle<'_>,
    ) -> Result<ArtifactReference, ArtifactError> {
        let input = record_input(bundle)?;
        let mut state = self.lock_state()?;
        require_trusted(&state)?;
        let now = self.effective_now(&state)?;
        self.remove_expired(&mut state, now)?;
        let record = Record::new(evaluation_id, now, self.config.retention, input)?;
        if let Some(id) = state.evaluations.get(evaluation_id.as_str()) {
            let existing = state.records.get(id).ok_or(ArtifactError::Corrupt)?;
            return if existing.metadata.id == record.id {
                existing.metadata.reference(&self.config)
            } else {
                Err(ArtifactError::Conflict)
            };
        }
        if state.records.contains_key(&record.id) {
            return Err(ArtifactError::Conflict);
        }
        let metadata_bytes = format::encode_record(&record)?;
        let record_bytes = record.blobs().try_fold(
            u64::try_from(metadata_bytes.len()).map_err(|_defect| ArtifactError::TooLarge)?,
            |total, (_component, blob)| {
                total
                    .checked_add(blob.length)
                    .ok_or(ArtifactError::TooLarge)
            },
        )?;
        let record_count =
            u64::try_from(state.records.len()).map_err(|_defect| ArtifactError::Full)?;
        if record_bytes > self.config.max_record_bytes {
            return Err(ArtifactError::TooLarge);
        }
        if record_count >= self.config.max_records
            || state
                .bytes
                .checked_add(record_bytes)
                .is_none_or(|total| total > self.config.max_bytes)
        {
            return Err(ArtifactError::Full);
        }
        self.advance_clock(&mut state, now)?;
        let payloads = bundle_payloads(bundle);
        if let Err(error) = self.write_record(&record, &metadata_bytes, payloads) {
            state.trusted = false;
            return Err(error);
        }
        state.bytes = state
            .bytes
            .checked_add(record_bytes)
            .ok_or(ArtifactError::Corrupt)?;
        state
            .evaluations
            .insert(evaluation_id.as_str().to_owned(), record.id.clone());
        let reference = record.reference(&self.config)?;
        state.records.insert(
            record.id.clone(),
            StoredRecord {
                metadata: record,
                bytes: record_bytes,
            },
        );
        Ok(reference)
    }

    /// Verifies that one staged locator is still live and every bound file is exact.
    ///
    /// # Errors
    ///
    /// The locator expired, was rebound, or its storage can no longer be trusted.
    pub fn verify(&self, reference: &ArtifactReference) -> Result<(), ArtifactError> {
        let mut state = self.lock_state()?;
        require_trusted(&state)?;
        let now = self.effective_now(&state)?;
        self.remove_expired(&mut state, now)?;
        let stored = state
            .records
            .get(&reference.id)
            .ok_or(ArtifactError::NotFound)?;
        if stored.metadata.reference(&self.config)? != *reference {
            return Err(ArtifactError::Conflict);
        }
        stored.metadata.blobs().try_for_each(|(component, blob)| {
            read_blob(
                &component_path(&self.root, &stored.metadata.id, component),
                blob,
            )
            .map(|_bytes| ())
        })
    }

    /// Finds the still-live artifact retained for one completed evaluation.
    ///
    /// # Errors
    ///
    /// The clock, root, or retained payloads cannot be trusted.
    pub fn find(
        &self,
        evaluation_id: &ControllerEvaluationId,
    ) -> Result<Option<ArtifactReference>, ArtifactError> {
        let mut state = self.lock_state()?;
        require_trusted(&state)?;
        let now = self.effective_now(&state)?;
        self.remove_expired(&mut state, now)?;
        state
            .evaluations
            .get(evaluation_id.as_str())
            .and_then(|id| state.records.get(id))
            .map(|stored| stored.metadata.reference(&self.config))
            .transpose()
    }

    /// Reads one exact retained component after enforcing its lifetime.
    ///
    /// # Errors
    ///
    /// The artifact or component is absent, expired, oversized, or corrupt.
    pub fn read(&self, id: &str, component: ArtifactComponent) -> Result<Vec<u8>, ArtifactError> {
        if !format::valid_id(id) {
            return Err(ArtifactError::NotFound);
        }
        let mut state = self.lock_state()?;
        require_trusted(&state)?;
        let now = self.effective_now(&state)?;
        self.remove_expired(&mut state, now)?;
        let record = &state
            .records
            .get(id)
            .ok_or(ArtifactError::NotFound)?
            .metadata;
        let blob = record
            .blobs()
            .find_map(|(candidate, blob)| (candidate == component).then_some(blob))
            .ok_or(ArtifactError::NotFound)?;
        read_blob(&component_path(&self.root, id, component), blob)
    }

    /// Removes every expired artifact and returns the exact released capacity.
    ///
    /// # Errors
    ///
    /// Trusted time or durable deletion cannot be established.
    pub fn cleanup(&self) -> Result<ArtifactCleanup, ArtifactError> {
        let mut state = self.lock_state()?;
        require_trusted(&state)?;
        let now = self.effective_now(&state)?;
        self.remove_expired(&mut state, now)
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, State>, ArtifactError> {
        self.state.lock().map_err(|_defect| ArtifactError::Corrupt)
    }

    fn effective_now(&self, state: &State) -> Result<i64, ArtifactError> {
        Ok(trusted_now(self.clock.as_ref())?.max(state.root.clock_high_water_unix_millis))
    }

    fn advance_clock(&self, state: &mut State, now: i64) -> Result<(), ArtifactError> {
        if now == state.root.clock_high_water_unix_millis {
            return Ok(());
        }
        state.root.clock_high_water_unix_millis = now;
        if let Err(error) = save_root(&self.root, &state.root) {
            state.trusted = false;
            return Err(error);
        }
        Ok(())
    }

    fn remove_expired(
        &self,
        state: &mut State,
        now: i64,
    ) -> Result<ArtifactCleanup, ArtifactError> {
        let expired = state
            .records
            .iter()
            .filter(|(_id, stored)| stored.metadata.expires_at_unix_millis <= now)
            .map(|(id, _stored)| id.clone())
            .collect::<Vec<_>>();
        if expired.is_empty() {
            return Ok(ArtifactCleanup::default());
        }
        self.advance_clock(state, now)?;
        let mut cleanup = ArtifactCleanup::default();
        for id in expired {
            let Some(stored) = state.records.get(&id) else {
                state.trusted = false;
                return Err(ArtifactError::Corrupt);
            };
            let evaluation_id = stored.metadata.evaluation_id.clone();
            let bytes = stored.bytes;
            if let Err(error) = remove_record(&self.root, &stored.metadata) {
                state.trusted = false;
                return Err(error);
            }
            state.records.remove(&id).ok_or(ArtifactError::Corrupt)?;
            state
                .evaluations
                .remove(&evaluation_id)
                .ok_or(ArtifactError::Corrupt)?;
            state.bytes = state
                .bytes
                .checked_sub(bytes)
                .ok_or(ArtifactError::Corrupt)?;
            cleanup.removed_records = cleanup
                .removed_records
                .checked_add(1)
                .ok_or(ArtifactError::Corrupt)?;
            cleanup.removed_bytes = cleanup
                .removed_bytes
                .checked_add(bytes)
                .ok_or(ArtifactError::Corrupt)?;
        }
        Ok(cleanup)
    }

    fn write_record(
        &self,
        record: &Record,
        metadata: &[u8],
        payloads: [(ArtifactComponent, Option<&[u8]>); 4],
    ) -> Result<(), ArtifactError> {
        let mut written = Vec::new();
        for (component, bytes) in payloads {
            let Some(bytes) = bytes else { continue };
            let path = component_path(&self.root, &record.id, component);
            if let Err(error) = atomic_write_new(&path, bytes) {
                remove_written(&written)?;
                return Err(error);
            }
            written.push(path);
        }
        let metadata_path = metadata_path(&self.root, &record.id);
        if let Err(error) = atomic_write_new(&metadata_path, metadata) {
            remove_written(&written)?;
            return Err(error);
        }
        Ok(())
    }
}

fn validate_config(config: &ArtifactStoreConfig) -> Result<(), ArtifactError> {
    let valid = super::artifact_route(&config.base_url).is_some()
        && !config.retention.is_zero()
        && config.retention <= super::MAX_ARTIFACT_RETENTION
        && (1..=super::MAX_ARTIFACT_RECORDS).contains(&config.max_records)
        && (1..=super::MAX_ARTIFACT_BYTES).contains(&config.max_bytes)
        && (1..=super::MAX_ARTIFACT_RECORD_BYTES).contains(&config.max_record_bytes)
        && config.max_record_bytes <= config.max_bytes;
    valid.then_some(()).ok_or(ArtifactError::Configuration)
}

fn record_input(bundle: ArtifactBundle<'_>) -> Result<RecordInput, ArtifactError> {
    let valid = bundle.assessment.is_some() == bundle.external_tally.is_some()
        && (!bundle.external_incomplete || bundle.assessment.is_none())
        && bundle
            .assessment
            .is_none_or(|_assessment| bundle.plan.is_some() && bundle.evidence.is_some());
    if !valid {
        return Err(ArtifactError::Corrupt);
    }
    Ok(RecordInput {
        report: Blob::new(bundle.report)?,
        plan: bundle.plan.map(Blob::new).transpose()?,
        evidence: bundle.evidence.map(Blob::new).transpose()?,
        assessment: bundle.assessment.map(Blob::new).transpose()?,
        external_tally: bundle.external_tally,
        external_incomplete: bundle.external_incomplete,
    })
}

fn bundle_payloads(bundle: ArtifactBundle<'_>) -> [(ArtifactComponent, Option<&[u8]>); 4] {
    [
        (ArtifactComponent::Report, Some(bundle.report)),
        (ArtifactComponent::Plan, bundle.plan),
        (ArtifactComponent::Evidence, bundle.evidence),
        (ArtifactComponent::Assessment, bundle.assessment),
    ]
}

fn scan(
    root: &Path,
    config: &ArtifactStoreConfig,
    root_state: Root,
) -> Result<State, ArtifactError> {
    let mut records = BTreeMap::new();
    let mut payloads = BTreeMap::new();
    let mut temporary = Vec::new();
    let maximum_entries = config
        .max_records
        .checked_mul(5)
        .and_then(|count| count.checked_add(3))
        .ok_or(ArtifactError::Configuration)?;
    for (position, entry) in fs::read_dir(root)?.enumerate() {
        if u64::try_from(position).unwrap_or(u64::MAX) >= maximum_entries {
            return Err(ArtifactError::Corrupt);
        }
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(ArtifactError::Corrupt)?;
        let file_type = entry.file_type()?;
        if matches!(name, OWNER_LOCK | ROOT_STATE) {
            if !file_type.is_file() {
                return Err(ArtifactError::Corrupt);
            }
        } else if let Some(id) = metadata_id(name) {
            if !file_type.is_file() || records.contains_key(id) {
                return Err(ArtifactError::Corrupt);
            }
            let bytes = read_bounded(&entry.path(), format::MAX_RECORD_METADATA_BYTES)?;
            let record = format::decode_record(&bytes)?;
            record.validate(config.retention)?;
            if record.id != id {
                return Err(ArtifactError::Corrupt);
            }
            records.insert(
                id.to_owned(),
                StoredRecord {
                    metadata: record,
                    bytes: u64::try_from(bytes.len()).map_err(|_defect| ArtifactError::Corrupt)?,
                },
            );
        } else if let Some((id, component)) = component_id(name) {
            if !file_type.is_file()
                || payloads
                    .insert((id.to_owned(), component), entry.path())
                    .is_some()
            {
                return Err(ArtifactError::Corrupt);
            }
        } else if name.starts_with(ATOMIC_WRITE_DIRECTORY_PREFIX) && file_type.is_dir() {
            temporary.push(recovered_directory(entry.path())?);
        } else {
            return Err(ArtifactError::Corrupt);
        }
    }
    for directory in temporary {
        directory.remove()?;
    }
    let mut evaluations = BTreeMap::new();
    let mut total = 0_u64;
    for (id, stored) in &mut records {
        if evaluations
            .insert(stored.metadata.evaluation_id.clone(), id.clone())
            .is_some()
        {
            return Err(ArtifactError::Corrupt);
        }
        for (component, blob) in stored.metadata.blobs() {
            let path = payloads
                .remove(&(id.clone(), component))
                .ok_or(ArtifactError::Corrupt)?;
            read_blob(&path, blob)?;
            stored.bytes = stored
                .bytes
                .checked_add(blob.length)
                .ok_or(ArtifactError::Corrupt)?;
        }
        if stored.bytes > config.max_record_bytes {
            return Err(ArtifactError::Corrupt);
        }
        total = total
            .checked_add(stored.bytes)
            .filter(|bytes| *bytes <= config.max_bytes)
            .ok_or(ArtifactError::Corrupt)?;
    }
    for path in payloads.into_values() {
        fs::remove_file(path)?;
    }
    if u64::try_from(records.len()).unwrap_or(u64::MAX) > config.max_records {
        return Err(ArtifactError::Corrupt);
    }
    Ok(State {
        root: root_state,
        records,
        evaluations,
        bytes: total,
        trusted: true,
    })
}

fn load_or_create_root(
    root: &Path,
    config: &ArtifactStoreConfig,
    now: i64,
) -> Result<Root, ArtifactError> {
    let path = root.join(ROOT_STATE);
    match read_bounded(&path, format::MAX_ROOT_BYTES) {
        Ok(bytes) => format::decode_root(&bytes),
        Err(ArtifactError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            prepare_new_root(root)?;
            let state = Root {
                schema: format::ROOT_SCHEMA.to_owned(),
                base_url: config.base_url.clone(),
                retention_millis: format::millis(config.retention)?,
                max_records: config.max_records,
                max_bytes: config.max_bytes,
                max_record_bytes: config.max_record_bytes,
                clock_high_water_unix_millis: now,
            };
            save_root(root, &state)?;
            Ok(state)
        }
        Err(error) => Err(error),
    }
}

fn prepare_new_root(root: &Path) -> Result<(), ArtifactError> {
    let mut temporary = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(ArtifactError::Corrupt)?;
        let file_type = entry.file_type()?;
        if name == OWNER_LOCK && file_type.is_file() {
            continue;
        }
        if name.starts_with(ATOMIC_WRITE_DIRECTORY_PREFIX) && file_type.is_dir() {
            temporary.push(recovered_directory(entry.path())?);
            continue;
        }
        return Err(ArtifactError::Corrupt);
    }
    for directory in temporary {
        directory.remove()?;
    }
    Ok(())
}

fn save_root(root: &Path, state: &Root) -> Result<(), ArtifactError> {
    atomic_write(&root.join(ROOT_STATE), &format::encode_root(state)?)
}

fn remove_record(root: &Path, record: &Record) -> Result<(), ArtifactError> {
    fs::remove_file(metadata_path(root, &record.id))?;
    record.blobs().try_for_each(|(component, _blob)| {
        fs::remove_file(component_path(root, &record.id, component)).map_err(Into::into)
    })
}

fn remove_written(paths: &[PathBuf]) -> Result<(), ArtifactError> {
    for path in paths {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn read_blob(path: &Path, blob: &Blob) -> Result<Vec<u8>, ArtifactError> {
    let bytes = read_bounded(path, blob.length)?;
    if u64::try_from(bytes.len()).ok() != Some(blob.length)
        || amiss_wire::digest::sha256(&bytes).to_string() != blob.digest
    {
        return Err(ArtifactError::Corrupt);
    }
    Ok(bytes)
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, ArtifactError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > maximum {
        return Err(ArtifactError::Corrupt);
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(ArtifactError::Corrupt);
    }
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(ArtifactError::Corrupt);
    }
    Ok(bytes)
}

fn open_lock(path: &Path) -> Result<File, ArtifactError> {
    reject_non_file(path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if !lock.metadata()?.is_file() {
        return Err(ArtifactError::Corrupt);
    }
    Ok(lock)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ArtifactError> {
    reject_non_file(path)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(io::Error::from)?;
    Ok(())
}

fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<(), ArtifactError> {
    reject_absent(path)?;
    AtomicFile::new(path, DisallowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(io::Error::from)?;
    Ok(())
}

fn reject_non_file(path: &Path) -> Result<(), ArtifactError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(ArtifactError::Corrupt),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn reject_absent(path: &Path) -> Result<(), ArtifactError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(ArtifactError::Conflict),
        Err(error) => Err(error.into()),
    }
}

fn trusted_now(clock: &dyn ControllerClock) -> Result<i64, ArtifactError> {
    clock
        .now_unix_millis()
        .filter(|now| *now >= 0)
        .ok_or(ArtifactError::Clock)
}

fn recovered_directory(path: PathBuf) -> Result<AtomicWriteDirectory, ArtifactError> {
    match AtomicWriteDirectory::read(path) {
        Ok(directory) => Ok(directory),
        Err(crate::atomic_write_recovery::AtomicWriteDirectoryError::Io(error)) => {
            Err(error.into())
        }
        Err(crate::atomic_write_recovery::AtomicWriteDirectoryError::Malformed) => {
            Err(ArtifactError::Corrupt)
        }
    }
}

fn require_trusted(state: &State) -> Result<(), ArtifactError> {
    state.trusted.then_some(()).ok_or(ArtifactError::Corrupt)
}

fn metadata_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.artifact"))
}

fn component_path(root: &Path, id: &str, component: ArtifactComponent) -> PathBuf {
    root.join(format!("{id}.{}", component_suffix(component)))
}

const fn component_suffix(component: ArtifactComponent) -> &'static str {
    match component {
        ArtifactComponent::Report => "report",
        ArtifactComponent::Plan => "plan",
        ArtifactComponent::Evidence => "evidence",
        ArtifactComponent::Assessment => "assessment",
    }
}

fn metadata_id(name: &str) -> Option<&str> {
    name.strip_suffix(".artifact")
        .filter(|id| format::valid_id(id))
}

fn component_id(name: &str) -> Option<(&str, ArtifactComponent)> {
    [
        ArtifactComponent::Report,
        ArtifactComponent::Plan,
        ArtifactComponent::Evidence,
        ArtifactComponent::Assessment,
    ]
    .into_iter()
    .find_map(|component| {
        name.strip_suffix(&format!(".{}", component_suffix(component)))
            .filter(|id| format::valid_id(id))
            .map(|id| (id, component))
    })
}
