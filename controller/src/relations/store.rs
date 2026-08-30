mod binding;
mod outbox;
mod status;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::ArtifactId;
use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};

use self::binding::{checked_work, pending_from_binding};
use self::status::{StoredStatus, validate_stored_status};
use super::{
    PendingRelation, RelationAdmission, RelationScheduleError, RelationStatusError,
    RelationTransition, schedule_relation,
};
use crate::atomic_write_recovery::{ATOMIC_WRITE_DIRECTORY_PREFIX, AtomicWriteDirectory};
use crate::file_ledger::{FileLedgerError, frame};

const LOCK_FILE: &str = ".amiss-relation-schedules.lock";
const METADATA_FILE: &str = ".amiss-relation-schedules.root";
const JOURNAL_FILE: &str = ".amiss-relation-schedules.journal";
const ROOT_SCHEMA: &str = "amiss/controller-relation-journal-root-v3";
const ENTRY_SCHEMA: &str = "amiss/controller-relation-journal-entry-v3";
const JOURNAL_CHAIN_DOMAIN: &str = "amiss/controller-relation-journal-chain-v3";
const MAX_ROOT_BYTES: u64 = 4_096;
const MAX_ENTRY_BYTES: u64 = 16_384;
const MAX_SMALL_ENTRY_BYTES: u64 = 4_096;
const ENTRY_LENGTH_BYTES: u64 = 8;
const MAX_ROOT_ENTRIES: usize = 16;

const ROOT_FRAME: frame::FrameFormat = frame::define(
    b"AMISS-RELATION-JOURNAL-ROOT",
    "amiss/controller-relation-journal-root-frame-v3",
    MAX_ROOT_BYTES,
);
const ENTRY_FRAME: frame::FrameFormat = frame::define(
    b"AMISS-RELATION-JOURNAL-ENTRY",
    "amiss/controller-relation-journal-entry-frame-v3",
    MAX_ENTRY_BYTES,
);

pub const RELATION_SCHEDULE_BINDING_LIMIT: u64 = 16_384;
const MAX_JOURNAL_BYTES_PER_BINDING: u64 =
    MAX_ENTRY_BYTES + (4 * MAX_SMALL_ENTRY_BYTES) + (5 * ENTRY_LENGTH_BYTES);
const MAX_ACTIONS_PER_BINDING: u64 = 5;

#[derive(Debug, thiserror::Error)]
pub enum RelationScheduleStoreError {
    #[error("relation schedule storage configuration differs")]
    Configuration,
    #[error("relation schedule storage is full")]
    Full,
    #[error("relation schedule storage is corrupt")]
    Corrupt,
    #[error("relation scheduling was refused: {0}")]
    Schedule(#[source] RelationScheduleError),
    #[error("relation status staging was refused: {0}")]
    Status(#[source] RelationStatusError),
    #[error("the retained relation audit is unavailable: {0}")]
    Artifact(#[source] crate::ArtifactError),
    #[error("not every relation status destination has been acknowledged")]
    DeliveryPending,
    #[error("relation schedule storage I/O failed: {0}")]
    Io(#[source] io::Error),
}

#[derive(Clone)]
pub struct FileRelationScheduleStore {
    root: PathBuf,
    max_bindings: u64,
    state: Arc<Mutex<State>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RootMetadata {
    schema: String,
    max_bindings: u64,
    binding_count: u64,
    entry_count: u64,
    journal_bytes: u64,
    tail_digest: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct JournalEntry {
    schema: String,
    previous_tail: Option<String>,
    action: JournalAction,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum JournalAction {
    Schedule {
        relation: String,
        plan_binding: String,
        binding: StoredBinding,
    },
    Stage {
        plan_binding: String,
        work_binding: String,
        status: Box<StoredStatus>,
    },
    Acknowledge {
        relation: String,
        coordination: String,
        status_binding: String,
        destination_binding: String,
    },
    Complete {
        relation: String,
        coordination: String,
        status_binding: String,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredBinding {
    coordination: String,
    work_binding: String,
    trigger_role: String,
    fence: u64,
}

#[derive(Default)]
struct State {
    relations: BTreeMap<String, StoredRelation>,
    statuses: BTreeMap<(String, String), StoredStatusState>,
    binding_count: u64,
    entry_count: u64,
    journal_bytes: u64,
    tail_digest: Option<String>,
}

enum StoredStatusState {
    Staged {
        binding: String,
        status: Box<StoredStatus>,
        acknowledged: BTreeSet<String>,
    },
    Completed {
        binding: String,
    },
}

struct StoredRelation {
    plan_binding: String,
    bindings: BTreeMap<String, StoredBinding>,
    current_coordination: String,
}

impl FileRelationScheduleStore {
    /// Opens one bounded, process-safe scheduling root and validates its
    /// committed journal.
    ///
    /// # Errors
    ///
    /// The root, fixed capacity, retained state, or filesystem cannot be trusted.
    pub fn open(
        root: impl AsRef<Path>,
        max_bindings: u64,
    ) -> Result<Self, RelationScheduleStoreError> {
        if !(1..=RELATION_SCHEDULE_BINDING_LIMIT).contains(&max_bindings)
            || !fs::symlink_metadata(root.as_ref())
                .map_err(RelationScheduleStoreError::Io)?
                .file_type()
                .is_dir()
        {
            return Err(RelationScheduleStoreError::Configuration);
        }
        let root = fs::canonicalize(root).map_err(RelationScheduleStoreError::Io)?;
        if !fs::symlink_metadata(&root)
            .map_err(RelationScheduleStoreError::Io)?
            .file_type()
            .is_dir()
        {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        let store = Self {
            root,
            max_bindings,
            state: Arc::new(Mutex::new(State::default())),
        };
        let _lock = store.lock()?;
        let metadata = store.load_or_initialize()?;
        let mut journal = store.open_committed_journal(&metadata)?;
        {
            let mut state = store.state()?;
            synchronize(&mut state, &mut journal, &metadata, max_bindings)?;
        }
        Ok(store)
    }

    /// Atomically admits one exact transition, retaining every prior
    /// coordination binding so delayed retries cannot roll current work back.
    ///
    /// # Errors
    ///
    /// The transition violates scheduling law, capacity is exhausted, or the
    /// durable state cannot be trusted.
    pub fn schedule(
        &self,
        transition: RelationTransition,
    ) -> Result<RelationAdmission, RelationScheduleStoreError> {
        let checked = checked_work(transition)?;
        let _lock = self.lock()?;
        let metadata = self.load_metadata()?;
        let mut journal = self.open_committed_journal(&metadata)?;
        let mut state = self.state()?;
        synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;

        let stored = state.relations.get(&checked.relation);
        if stored.is_some_and(|stored| stored.plan_binding != checked.plan_binding) {
            return Err(RelationScheduleStoreError::Schedule(
                RelationScheduleError::BindingConflict,
            ));
        }
        if let Some(previous) =
            stored.and_then(|stored| stored.bindings.get(&checked.binding.coordination))
        {
            if previous.work_binding != checked.binding.work_binding {
                return Err(RelationScheduleStoreError::Schedule(
                    RelationScheduleError::CoordinationConflict,
                ));
            }
            return pending_from_binding(checked.transition, previous)
                .map(RelationAdmission::Duplicate);
        }
        if state.binding_count >= self.max_bindings {
            return Err(RelationScheduleStoreError::Full);
        }

        let previous = stored
            .map(|stored| {
                stored
                    .bindings
                    .get(&stored.current_coordination)
                    .ok_or(RelationScheduleStoreError::Corrupt)
            })
            .transpose()?
            .map(|binding| pending_from_binding(checked.transition.clone(), binding))
            .transpose()?;
        let pending = match schedule_relation(previous, checked.transition)
            .map_err(RelationScheduleStoreError::Schedule)?
        {
            RelationAdmission::Scheduled(pending) => pending,
            RelationAdmission::Duplicate(_) => return Err(RelationScheduleStoreError::Corrupt),
        };
        let mut binding = checked.binding;
        binding.fence = pending.fence.get();
        let previous_tail = state.tail_digest.clone();
        self.append(
            &mut journal,
            &metadata,
            &mut state,
            JournalEntry {
                schema: ENTRY_SCHEMA.to_owned(),
                previous_tail,
                action: JournalAction::Schedule {
                    relation: checked.relation,
                    plan_binding: checked.plan_binding,
                    binding,
                },
            },
        )?;
        Ok(RelationAdmission::Scheduled(pending))
    }

    /// Reports whether one worker still owns the latest exact fence.
    ///
    /// # Errors
    ///
    /// The pending value or durable state cannot be trusted.
    pub fn is_current(
        &self,
        pending: &PendingRelation,
    ) -> Result<bool, RelationScheduleStoreError> {
        let checked = checked_work(pending.transition.clone())?;
        let _lock = self.lock()?;
        let metadata = self.load_metadata()?;
        let mut journal = self.open_committed_journal(&metadata)?;
        let mut state = self.state()?;
        synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
        Ok(is_current_work(&state, &checked, pending))
    }

    fn append(
        &self,
        journal: &mut File,
        metadata: &RootMetadata,
        state: &mut State,
        entry: JournalEntry,
    ) -> Result<(), RelationScheduleStoreError> {
        let chunk = encode_entry(&entry)?;
        let chunk_length =
            u64::try_from(chunk.len()).map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
        let next_bytes = metadata
            .journal_bytes
            .checked_add(chunk_length)
            .filter(|bytes| {
                journal_byte_limit(self.max_bindings).is_some_and(|limit| *bytes <= limit)
            })
            .ok_or(RelationScheduleStoreError::Full)?;
        let binding_increment = u64::from(matches!(&entry.action, JournalAction::Schedule { .. }));
        let next_count = metadata
            .binding_count
            .checked_add(binding_increment)
            .filter(|count| *count <= self.max_bindings)
            .ok_or(RelationScheduleStoreError::Full)?;
        let max_entries = self
            .max_bindings
            .checked_mul(MAX_ACTIONS_PER_BINDING)
            .ok_or(RelationScheduleStoreError::Configuration)?;
        let next_entry_count = metadata
            .entry_count
            .checked_add(1)
            .filter(|count| *count <= max_entries)
            .ok_or(RelationScheduleStoreError::Full)?;
        let tail_digest = hb(JOURNAL_CHAIN_DOMAIN, &chunk).to_string();
        validate_append(state, &entry, chunk_length, self.max_bindings)?;
        if journal
            .seek(SeekFrom::End(0))
            .map_err(RelationScheduleStoreError::Io)?
            != metadata.journal_bytes
        {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        journal
            .write_all(&chunk)
            .and_then(|()| journal.sync_all())
            .map_err(RelationScheduleStoreError::Io)?;
        let next = RootMetadata {
            schema: ROOT_SCHEMA.to_owned(),
            max_bindings: self.max_bindings,
            binding_count: next_count,
            entry_count: next_entry_count,
            journal_bytes: next_bytes,
            tail_digest: Some(tail_digest.clone()),
        };
        save_metadata(&self.root, &next)?;
        apply_entry(state, entry, tail_digest, chunk_length, self.max_bindings)?;
        if !state.matches(&next) {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        Ok(())
    }

    fn state(&self) -> Result<MutexGuard<'_, State>, RelationScheduleStoreError> {
        self.state
            .lock()
            .map_err(|_defect| RelationScheduleStoreError::Corrupt)
    }

    fn lock(&self) -> Result<File, RelationScheduleStoreError> {
        let path = self.root.join(LOCK_FILE);
        reject_non_file(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(RelationScheduleStoreError::Io)?;
        if !file
            .metadata()
            .map_err(RelationScheduleStoreError::Io)?
            .is_file()
        {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        file.lock().map_err(RelationScheduleStoreError::Io)?;
        Ok(file)
    }

    fn load_or_initialize(&self) -> Result<RootMetadata, RelationScheduleStoreError> {
        let (metadata_present, journal_present) = scan_root(&self.root)?;
        match (metadata_present, journal_present) {
            (true, true) => self.load_metadata(),
            (false, false) => {
                create_journal(&self.root)?;
                let metadata = empty_metadata(self.max_bindings);
                save_metadata(&self.root, &metadata)?;
                Ok(metadata)
            }
            (false, true) => {
                if open_journal(&self.root)?
                    .metadata()
                    .map_err(RelationScheduleStoreError::Io)?
                    .len()
                    != 0
                {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
                let metadata = empty_metadata(self.max_bindings);
                save_metadata(&self.root, &metadata)?;
                Ok(metadata)
            }
            (true, false) => Err(RelationScheduleStoreError::Corrupt),
        }
    }

    fn load_metadata(&self) -> Result<RootMetadata, RelationScheduleStoreError> {
        let (metadata_present, journal_present) = scan_root(&self.root)?;
        if !metadata_present || !journal_present {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        let bytes =
            amiss_controller_files::read_bounded(&self.root.join(METADATA_FILE), MAX_ROOT_BYTES)
                .map_err(RelationScheduleStoreError::Io)?;
        let metadata = frame::decode(ROOT_FRAME, &bytes, |metadata| {
            validate_metadata(metadata).map_err(|_defect| FileLedgerError::Corrupt)
        })
        .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
        if metadata.max_bindings != self.max_bindings {
            return Err(RelationScheduleStoreError::Configuration);
        }
        Ok(metadata)
    }

    fn open_committed_journal(
        &self,
        metadata: &RootMetadata,
    ) -> Result<File, RelationScheduleStoreError> {
        let journal = open_journal(&self.root)?;
        let physical_bytes = journal
            .metadata()
            .map_err(RelationScheduleStoreError::Io)?
            .len();
        if physical_bytes < metadata.journal_bytes {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        if physical_bytes > metadata.journal_bytes {
            journal
                .set_len(metadata.journal_bytes)
                .and_then(|()| journal.sync_all())
                .map_err(RelationScheduleStoreError::Io)?;
        }
        Ok(journal)
    }
}

impl State {
    fn matches(&self, metadata: &RootMetadata) -> bool {
        self.binding_count == metadata.binding_count
            && self.entry_count == metadata.entry_count
            && self.journal_bytes == metadata.journal_bytes
            && self.tail_digest == metadata.tail_digest
    }
}

fn is_current_work(
    state: &State,
    checked: &binding::CheckedWork,
    pending: &PendingRelation,
) -> bool {
    state
        .relations
        .get(&checked.relation)
        .filter(|stored| stored.plan_binding == checked.plan_binding)
        .and_then(|stored| stored.bindings.get(&stored.current_coordination))
        .is_some_and(|current| {
            current.coordination == checked.binding.coordination
                && current.work_binding == checked.binding.work_binding
                && current.trigger_role == checked.binding.trigger_role
                && current.fence == pending.fence.get()
        })
}

fn synchronize(
    state: &mut State,
    journal: &mut File,
    metadata: &RootMetadata,
    max_bindings: u64,
) -> Result<(), RelationScheduleStoreError> {
    if state.matches(metadata) {
        return Ok(());
    }
    if state.journal_bytes > metadata.journal_bytes
        || state.binding_count > metadata.binding_count
        || state.entry_count > metadata.entry_count
    {
        *state = State::default();
    }
    read_entries(journal, state, metadata.journal_bytes, max_bindings)?;
    if !state.matches(metadata) {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    Ok(())
}

fn read_entries(
    journal: &mut File,
    state: &mut State,
    committed_bytes: u64,
    max_bindings: u64,
) -> Result<(), RelationScheduleStoreError> {
    journal
        .seek(SeekFrom::Start(state.journal_bytes))
        .map_err(RelationScheduleStoreError::Io)?;
    while state.journal_bytes < committed_bytes {
        let remaining = committed_bytes
            .checked_sub(state.journal_bytes)
            .ok_or(RelationScheduleStoreError::Corrupt)?;
        if remaining < ENTRY_LENGTH_BYTES {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        let mut length_bytes = [0_u8; 8];
        journal
            .read_exact(&mut length_bytes)
            .map_err(RelationScheduleStoreError::Io)?;
        let frame_length = u64::from_be_bytes(length_bytes);
        let chunk_length = ENTRY_LENGTH_BYTES
            .checked_add(frame_length)
            .filter(|length| *length <= remaining)
            .ok_or(RelationScheduleStoreError::Corrupt)?;
        if frame_length == 0 || frame_length > MAX_ENTRY_BYTES {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        let frame_length =
            usize::try_from(frame_length).map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
        let chunk_size = frame_length
            .checked_add(length_bytes.len())
            .ok_or(RelationScheduleStoreError::Corrupt)?;
        let mut chunk = Vec::with_capacity(chunk_size);
        chunk.extend_from_slice(&length_bytes);
        chunk.resize(chunk_size, 0);
        journal
            .read_exact(
                chunk
                    .get_mut(length_bytes.len()..)
                    .ok_or(RelationScheduleStoreError::Corrupt)?,
            )
            .map_err(RelationScheduleStoreError::Io)?;
        let entry = decode_entry(
            chunk
                .get(length_bytes.len()..)
                .ok_or(RelationScheduleStoreError::Corrupt)?,
        )?;
        let tail_digest = hb(JOURNAL_CHAIN_DOMAIN, &chunk).to_string();
        apply_entry(state, entry, tail_digest, chunk_length, max_bindings)?;
    }
    Ok(())
}

fn validate_append(
    state: &State,
    entry: &JournalEntry,
    chunk_length: u64,
    max_bindings: u64,
) -> Result<(), RelationScheduleStoreError> {
    let max_entries = max_bindings
        .checked_mul(MAX_ACTIONS_PER_BINDING)
        .ok_or(RelationScheduleStoreError::Corrupt)?;
    let journal_byte_limit =
        journal_byte_limit(max_bindings).ok_or(RelationScheduleStoreError::Corrupt)?;
    if entry.previous_tail != state.tail_digest
        || state.entry_count >= max_entries
        || state
            .journal_bytes
            .checked_add(chunk_length)
            .is_none_or(|bytes| bytes > journal_byte_limit)
    {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    match &entry.action {
        JournalAction::Schedule {
            relation,
            plan_binding,
            binding,
        } => match state.relations.get(relation) {
            None if binding.fence == 1 && state.binding_count < max_bindings => Ok(()),
            Some(relation)
                if state.binding_count < max_bindings
                    && relation.plan_binding == *plan_binding
                    && !relation.bindings.contains_key(&binding.coordination)
                    && relation
                        .bindings
                        .get(&relation.current_coordination)
                        .and_then(|binding| binding.fence.checked_add(1))
                        == Some(binding.fence) =>
            {
                Ok(())
            }
            None | Some(_) => Err(RelationScheduleStoreError::Corrupt),
        },
        JournalAction::Stage {
            plan_binding,
            work_binding,
            status,
        } => {
            validate_stored_status(status)?;
            let key = (status.relation.clone(), status.coordination.clone());
            let current = state
                .relations
                .get(&status.relation)
                .filter(|relation| {
                    relation.plan_binding == *plan_binding
                        && relation.current_coordination == status.coordination
                })
                .and_then(|relation| relation.bindings.get(&status.coordination));
            if state.statuses.contains_key(&key)
                || current.is_none_or(|binding| {
                    binding.work_binding != *work_binding
                        || binding.trigger_role != status.trigger_role
                        || binding.fence != status.fence
                })
            {
                Err(RelationScheduleStoreError::Corrupt)
            } else {
                Ok(())
            }
        }
        JournalAction::Acknowledge {
            relation,
            coordination,
            status_binding,
            destination_binding,
        } => validate_status_progress(
            state,
            relation,
            coordination,
            status_binding,
            Some(destination_binding),
        ),
        JournalAction::Complete {
            relation,
            coordination,
            status_binding,
        } => validate_status_progress(state, relation, coordination, status_binding, None),
    }
}

fn validate_status_progress(
    state: &State,
    relation: &str,
    coordination: &str,
    status_binding: &str,
    destination: Option<&str>,
) -> Result<(), RelationScheduleStoreError> {
    let Some(StoredStatusState::Staged {
        binding,
        status,
        acknowledged,
    }) = state
        .statuses
        .get(&(relation.to_owned(), coordination.to_owned()))
    else {
        return Err(RelationScheduleStoreError::Corrupt);
    };
    if binding != status_binding {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    let valid = match destination {
        Some(destination) => {
            status
                .destinations
                .binary_search_by(|stored| stored.as_str().cmp(destination))
                .is_ok()
                && !acknowledged.contains(destination)
        }
        None => {
            status.destinations.len() == acknowledged.len()
                && status
                    .destinations
                    .iter()
                    .all(|destination| acknowledged.contains(destination))
        }
    };
    valid
        .then_some(())
        .ok_or(RelationScheduleStoreError::Corrupt)
}

fn apply_entry(
    state: &mut State,
    entry: JournalEntry,
    tail_digest: String,
    chunk_length: u64,
    max_bindings: u64,
) -> Result<(), RelationScheduleStoreError> {
    validate_append(state, &entry, chunk_length, max_bindings)?;
    let binding_count = state
        .binding_count
        .checked_add(u64::from(matches!(
            &entry.action,
            JournalAction::Schedule { .. }
        )))
        .ok_or(RelationScheduleStoreError::Corrupt)?;
    let entry_count = state
        .entry_count
        .checked_add(1)
        .ok_or(RelationScheduleStoreError::Corrupt)?;
    let journal_bytes = state
        .journal_bytes
        .checked_add(chunk_length)
        .ok_or(RelationScheduleStoreError::Corrupt)?;
    match entry.action {
        JournalAction::Schedule {
            relation,
            plan_binding,
            binding,
        } => {
            let coordination = binding.coordination.clone();
            match state.relations.entry(relation) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(StoredRelation {
                        plan_binding,
                        bindings: BTreeMap::from([(coordination.clone(), binding)]),
                        current_coordination: coordination,
                    });
                }
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    let relation = slot.get_mut();
                    relation.bindings.insert(coordination.clone(), binding);
                    relation.current_coordination = coordination;
                }
            }
        }
        JournalAction::Stage { status, .. } => {
            let key = (status.relation.clone(), status.coordination.clone());
            let binding = status.status_binding.clone();
            state.statuses.insert(
                key,
                StoredStatusState::Staged {
                    binding,
                    status,
                    acknowledged: BTreeSet::new(),
                },
            );
        }
        JournalAction::Acknowledge {
            relation,
            coordination,
            destination_binding,
            ..
        } => {
            let Some(StoredStatusState::Staged { acknowledged, .. }) =
                state.statuses.get_mut(&(relation, coordination))
            else {
                return Err(RelationScheduleStoreError::Corrupt);
            };
            if !acknowledged.insert(destination_binding) {
                return Err(RelationScheduleStoreError::Corrupt);
            }
        }
        JournalAction::Complete {
            relation,
            coordination,
            status_binding: binding,
        } => {
            state.statuses.insert(
                (relation, coordination),
                StoredStatusState::Completed { binding },
            );
        }
    }
    state.binding_count = binding_count;
    state.entry_count = entry_count;
    state.journal_bytes = journal_bytes;
    state.tail_digest = Some(tail_digest);
    Ok(())
}

fn empty_metadata(max_bindings: u64) -> RootMetadata {
    RootMetadata {
        schema: ROOT_SCHEMA.to_owned(),
        max_bindings,
        binding_count: 0,
        entry_count: 0,
        journal_bytes: 0,
        tail_digest: None,
    }
}

fn validate_metadata(metadata: &RootMetadata) -> Result<(), RelationScheduleStoreError> {
    let empty = metadata.entry_count == 0;
    let max_entries = metadata
        .max_bindings
        .checked_mul(MAX_ACTIONS_PER_BINDING)
        .ok_or(RelationScheduleStoreError::Corrupt)?;
    let admitted_entries = metadata
        .binding_count
        .checked_mul(MAX_ACTIONS_PER_BINDING)
        .ok_or(RelationScheduleStoreError::Corrupt)?;
    let journal_byte_limit =
        journal_byte_limit(metadata.max_bindings).ok_or(RelationScheduleStoreError::Corrupt)?;
    if metadata.schema != ROOT_SCHEMA
        || !(1..=RELATION_SCHEDULE_BINDING_LIMIT).contains(&metadata.max_bindings)
        || metadata.binding_count > metadata.max_bindings
        || metadata.entry_count < metadata.binding_count
        || metadata.entry_count > max_entries
        || metadata.entry_count > admitted_entries
        || metadata.journal_bytes > journal_byte_limit
        || (metadata.journal_bytes == 0) != empty
        || metadata.tail_digest.is_none() != empty
        || metadata
            .tail_digest
            .as_deref()
            .is_some_and(|digest| Digest::from_wire(digest).is_none())
    {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    Ok(())
}

const fn journal_byte_limit(max_bindings: u64) -> Option<u64> {
    max_bindings.checked_mul(MAX_JOURNAL_BYTES_PER_BINDING)
}

fn validate_entry(entry: &JournalEntry) -> Result<(), RelationScheduleStoreError> {
    if entry.schema != ENTRY_SCHEMA
        || entry
            .previous_tail
            .as_deref()
            .is_some_and(|digest| Digest::from_wire(digest).is_none())
    {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    let valid = match &entry.action {
        JournalAction::Schedule {
            relation,
            plan_binding,
            binding,
        } => {
            ArtifactId::new(relation.clone()).is_some()
                && Digest::from_wire(plan_binding).is_some()
                && ArtifactId::new(binding.coordination.clone()).is_some()
                && Digest::from_wire(&binding.work_binding).is_some()
                && ArtifactId::new(binding.trigger_role.clone()).is_some()
                && binding.fence != 0
        }
        JournalAction::Stage {
            plan_binding,
            work_binding,
            status,
        } => {
            Digest::from_wire(plan_binding).is_some()
                && Digest::from_wire(work_binding).is_some()
                && validate_stored_status(status).is_ok()
        }
        JournalAction::Acknowledge {
            relation,
            coordination,
            status_binding,
            destination_binding,
        } => {
            ArtifactId::new(relation.clone()).is_some()
                && ArtifactId::new(coordination.clone()).is_some()
                && Digest::from_wire(status_binding).is_some()
                && Digest::from_wire(destination_binding).is_some()
        }
        JournalAction::Complete {
            relation,
            coordination,
            status_binding,
        } => {
            ArtifactId::new(relation.clone()).is_some()
                && ArtifactId::new(coordination.clone()).is_some()
                && Digest::from_wire(status_binding).is_some()
        }
    };
    valid
        .then_some(())
        .ok_or(RelationScheduleStoreError::Corrupt)
}

fn encode_entry(entry: &JournalEntry) -> Result<Vec<u8>, RelationScheduleStoreError> {
    let frame = frame::encode(ENTRY_FRAME, entry, |entry| {
        validate_entry(entry).map_err(|_defect| FileLedgerError::Corrupt)
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    let length =
        u64::try_from(frame.len()).map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    if length > entry_byte_limit(&entry.action) {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    let mut chunk = Vec::with_capacity(
        frame
            .len()
            .checked_add(8)
            .ok_or(RelationScheduleStoreError::Corrupt)?,
    );
    chunk.extend_from_slice(&length.to_be_bytes());
    chunk.extend_from_slice(&frame);
    Ok(chunk)
}

fn decode_entry(bytes: &[u8]) -> Result<JournalEntry, RelationScheduleStoreError> {
    let entry = frame::decode(ENTRY_FRAME, bytes, |entry| {
        validate_entry(entry).map_err(|_defect| FileLedgerError::Corrupt)
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    let length =
        u64::try_from(bytes.len()).map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    (length <= entry_byte_limit(&entry.action))
        .then_some(entry)
        .ok_or(RelationScheduleStoreError::Corrupt)
}

const fn entry_byte_limit(action: &JournalAction) -> u64 {
    match action {
        JournalAction::Stage { .. } => MAX_ENTRY_BYTES,
        JournalAction::Schedule { .. }
        | JournalAction::Acknowledge { .. }
        | JournalAction::Complete { .. } => MAX_SMALL_ENTRY_BYTES,
    }
}

fn save_metadata(root: &Path, metadata: &RootMetadata) -> Result<(), RelationScheduleStoreError> {
    let bytes = frame::encode(ROOT_FRAME, metadata, |metadata| {
        validate_metadata(metadata).map_err(|_defect| FileLedgerError::Corrupt)
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    atomic_write(&root.join(METADATA_FILE), &bytes)
}

fn scan_root(root: &Path) -> Result<(bool, bool), RelationScheduleStoreError> {
    let mut lock = false;
    let mut metadata = false;
    let mut journal = false;
    let mut temporary = Vec::new();
    for (position, entry) in fs::read_dir(root)
        .map_err(RelationScheduleStoreError::Io)?
        .enumerate()
    {
        if position >= MAX_ROOT_ENTRIES {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        let entry = entry.map_err(RelationScheduleStoreError::Io)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(RelationScheduleStoreError::Corrupt)?;
        let file_type = entry.file_type().map_err(RelationScheduleStoreError::Io)?;
        match name {
            LOCK_FILE if file_type.is_file() && !lock => lock = true,
            METADATA_FILE if file_type.is_file() && !metadata => metadata = true,
            JOURNAL_FILE if file_type.is_file() && !journal => journal = true,
            _ if name.starts_with(ATOMIC_WRITE_DIRECTORY_PREFIX) && file_type.is_dir() => {
                temporary.push(AtomicWriteDirectory::read(entry.path()).map_err(
                    |error| match error {
                        crate::atomic_write_recovery::AtomicWriteDirectoryError::Io(error) => {
                            RelationScheduleStoreError::Io(error)
                        }
                        crate::atomic_write_recovery::AtomicWriteDirectoryError::Malformed => {
                            RelationScheduleStoreError::Corrupt
                        }
                    },
                )?);
            }
            _ => return Err(RelationScheduleStoreError::Corrupt),
        }
    }
    if !lock {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    for directory in temporary {
        directory.remove().map_err(RelationScheduleStoreError::Io)?;
    }
    Ok((metadata, journal))
}

fn create_journal(root: &Path) -> Result<(), RelationScheduleStoreError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(root.join(JOURNAL_FILE))
        .map(|_journal| ())
        .map_err(RelationScheduleStoreError::Io)
}

fn open_journal(root: &Path) -> Result<File, RelationScheduleStoreError> {
    let path = root.join(JOURNAL_FILE);
    reject_non_file(&path)?;
    let journal = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(RelationScheduleStoreError::Io)?;
    if !journal
        .metadata()
        .map_err(RelationScheduleStoreError::Io)?
        .is_file()
    {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    Ok(journal)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RelationScheduleStoreError> {
    reject_non_file(path)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(bytes))
        .map_err(io::Error::from)
        .map_err(RelationScheduleStoreError::Io)
}

fn reject_non_file(path: &Path) -> Result<(), RelationScheduleStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(RelationScheduleStoreError::Corrupt),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RelationScheduleStoreError::Io(error)),
    }
}
