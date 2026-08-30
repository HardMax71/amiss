use crate::{
    ArtifactAuditReference, FileArtifactStore, PendingRelation, RelationAuditBundle,
    RelationStatusError, RelationStatusRecord, RelationSubjectHead, complete_relation_status,
    stage_relation_status,
};

use super::status::store_status;
use super::{
    ENTRY_SCHEMA, FileRelationScheduleStore, JournalAction, JournalEntry,
    RelationScheduleStoreError, StoredStatusState, checked_work, is_current_work, synchronize,
};

impl FileRelationScheduleStore {
    /// Atomically retains one exact status batch only while its complete
    /// relation work remains current. An unfinished exact retry returns the
    /// retained record and an exact completed retry returns `None`.
    ///
    /// This transition performs no provider I/O. A publisher must acquire a
    /// separate delivery authority before using the returned data externally.
    ///
    /// # Errors
    ///
    /// The schedule, audit artifact, final heads, prior status binding, or
    /// durable journal cannot be trusted.
    pub fn stage_status(
        &self,
        artifacts: &FileArtifactStore,
        pending: &PendingRelation,
        heads: [RelationSubjectHead; 2],
        audit: ArtifactAuditReference,
        bundle: RelationAuditBundle<'_>,
    ) -> Result<Option<RelationStatusRecord>, RelationScheduleStoreError> {
        let checked = checked_work(pending.transition.clone())?;
        let _lock = self.lock()?;
        let metadata = self.load_metadata()?;
        let mut journal = self.open_committed_journal(&metadata)?;
        let mut state = self.state()?;
        synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
        let record = stage_relation_status(
            pending,
            is_current_work(&state, &checked, pending).then_some(pending),
            heads,
            None,
            audit,
            bundle,
        )
        .map_err(RelationScheduleStoreError::Status)?
        .ok_or(RelationScheduleStoreError::Corrupt)?;
        let stored = store_status(&record)?;
        let binding = stored.status_binding.clone();
        let key = (stored.relation.clone(), stored.coordination.clone());
        match state.statuses.get(&key) {
            Some(StoredStatusState::Staged {
                binding: existing,
                status,
            }) if existing == &binding => {
                if status.as_ref() != &stored {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
                artifacts
                    .verify(&record.audit.artifact)
                    .map_err(RelationScheduleStoreError::Artifact)?;
                return Ok(Some(record));
            }
            Some(StoredStatusState::Completed { binding: existing }) if existing == &binding => {
                return Ok(None);
            }
            Some(_) => {
                return Err(RelationScheduleStoreError::Status(
                    RelationStatusError::BindingConflict,
                ));
            }
            None => {}
        }
        artifacts
            .verify(&record.audit.artifact)
            .map_err(RelationScheduleStoreError::Artifact)?;
        let previous_tail = state.tail_digest.clone();
        self.append(
            &mut journal,
            &metadata,
            &mut state,
            JournalEntry {
                schema: ENTRY_SCHEMA.to_owned(),
                previous_tail,
                action: JournalAction::Stage {
                    plan_binding: checked.plan_binding,
                    work_binding: checked.binding.work_binding,
                    status: Box::new(stored),
                },
            },
        )?;
        Ok(Some(record))
    }

    /// Atomically marks the exact staged status batch complete. Repeating an
    /// acknowledged completion for the same immutable record is successful.
    ///
    /// # Errors
    ///
    /// The record was never staged, was rebound, or durable state cannot be
    /// trusted.
    pub fn complete_status(
        &self,
        staged: &RelationStatusRecord,
    ) -> Result<RelationStatusRecord, RelationScheduleStoreError> {
        let stored = store_status(staged)?;
        let binding = stored.status_binding.clone();
        let key = (stored.relation.clone(), stored.coordination.clone());
        let _lock = self.lock()?;
        let metadata = self.load_metadata()?;
        let mut journal = self.open_committed_journal(&metadata)?;
        let mut state = self.state()?;
        synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
        match state.statuses.get(&key) {
            Some(StoredStatusState::Staged {
                binding: existing,
                status,
            }) if existing == &binding => {
                if status.as_ref() != &stored {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
            }
            Some(StoredStatusState::Completed { binding: existing }) if existing == &binding => {
                return complete_relation_status(staged, staged)
                    .map_err(RelationScheduleStoreError::Status);
            }
            Some(_) | None => {
                return Err(RelationScheduleStoreError::Status(
                    RelationStatusError::BindingConflict,
                ));
            }
        }
        let completed =
            complete_relation_status(staged, staged).map_err(RelationScheduleStoreError::Status)?;
        let previous_tail = state.tail_digest.clone();
        self.append(
            &mut journal,
            &metadata,
            &mut state,
            JournalEntry {
                schema: ENTRY_SCHEMA.to_owned(),
                previous_tail,
                action: JournalAction::Complete {
                    relation: key.0,
                    coordination: key.1,
                    status_binding: binding,
                },
            },
        )?;
        Ok(completed)
    }
}
