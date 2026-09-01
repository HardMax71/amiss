use amiss_wire::digest::Digest;
use amiss_wire::model::ArtifactId;

use crate::file_ledger::{FileLedgerError, frame};

use super::{
    JournalAction, JournalEntry, RELATION_SCHEDULE_BINDING_LIMIT, RelationScheduleStoreError,
    RootMetadata,
};

pub(super) const ROOT_SCHEMA: &str = "amiss/controller-relation-journal-root-v3";
pub(super) const ENTRY_SCHEMA: &str = "amiss/controller-relation-journal-entry-v3";
pub(super) const MAX_ROOT_BYTES: u64 = 4_096;
pub(super) const MAX_ENTRY_BYTES: u64 = 16_384;
const MAX_SMALL_ENTRY_BYTES: u64 = 4_096;
pub(super) const ENTRY_LENGTH_BYTES: u64 = 8;
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
const MAX_JOURNAL_BYTES_PER_BINDING: u64 =
    MAX_ENTRY_BYTES + (4 * MAX_SMALL_ENTRY_BYTES) + (5 * ENTRY_LENGTH_BYTES);
pub(super) const MAX_ACTIONS_PER_BINDING: u64 = 5;

pub(super) fn empty_metadata(max_bindings: u64) -> RootMetadata {
    RootMetadata {
        schema: ROOT_SCHEMA.to_owned(),
        max_bindings,
        binding_count: 0,
        entry_count: 0,
        journal_bytes: 0,
        tail_digest: None,
    }
}

pub(super) fn decode_metadata(
    bytes: &[u8],
    max_bindings: u64,
) -> Result<RootMetadata, RelationScheduleStoreError> {
    let metadata = frame::decode(ROOT_FRAME, bytes, |metadata| {
        validate_metadata(metadata).map_err(|_defect| FileLedgerError::Corrupt)
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    if metadata.max_bindings != max_bindings {
        return Err(RelationScheduleStoreError::Configuration);
    }
    Ok(metadata)
}

pub(super) fn encode_metadata(
    metadata: &RootMetadata,
) -> Result<Vec<u8>, RelationScheduleStoreError> {
    frame::encode(ROOT_FRAME, metadata, |metadata| {
        validate_metadata(metadata).map_err(|_defect| FileLedgerError::Corrupt)
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)
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

pub(super) const fn journal_byte_limit(max_bindings: u64) -> Option<u64> {
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
                && super::status::validate_stored_status(status).is_ok()
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

pub(super) fn encode_entry(entry: &JournalEntry) -> Result<Vec<u8>, RelationScheduleStoreError> {
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

pub(super) fn decode_entry(bytes: &[u8]) -> Result<JournalEntry, RelationScheduleStoreError> {
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
