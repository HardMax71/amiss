use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;

use amiss_wire::digest::Digest;

use crate::{
    ArtifactAuditReference, FileArtifactStore, PendingRelation, RelationAuditBundle,
    RelationRegistry, RelationStatusError, RelationStatusRecord, RelationStatusTarget,
    RelationSubjectHead, complete_relation_status, stage_relation_status,
};

use super::status::{StoredStatus, destination_binding, reopen_status, store_status};
use super::{
    FileRelationScheduleStore, JournalAction, RelationScheduleStoreError, RootMetadata, State,
    StoredStatusState, checked_work, is_current_work, synchronize,
};

/// One exact destination whose operating-system lock remains held until the
/// value is acknowledged or the claim is dropped.
pub struct RelationStatusDeliveryClaim {
    pub status: RelationStatusRecord,
    pub target: RelationStatusTarget,
    status_binding: String,
    destination_binding: String,
    _delivery_lock: File,
}

struct PendingDelivery {
    status: StoredStatus,
    destination: String,
    shard: u8,
}

struct PendingDeliveryRef<'a> {
    status: &'a StoredStatus,
    destination: &'a str,
    shard: u8,
}

impl FileRelationScheduleStore {
    /// Reopens one unfinished exact batch from its immutable registry and
    /// retained artifact after restart. The returned value is not external
    /// delivery authority.
    ///
    /// # Errors
    ///
    /// The journal, registry, retained audit, or reproduced binding cannot be
    /// trusted.
    pub fn reopen_staged_status(
        &self,
        registry: &RelationRegistry,
        artifacts: &FileArtifactStore,
        relation: &amiss_wire::model::ArtifactId,
        coordination: &amiss_wire::model::ArtifactId,
    ) -> Result<Option<RelationStatusRecord>, RelationScheduleStoreError> {
        let stored = self.load_staged_status(relation, coordination)?;
        stored
            .as_ref()
            .map(|(status, plan_binding)| reopen_status(status, plan_binding, registry, artifacts))
            .transpose()
    }

    fn load_staged_status(
        &self,
        relation: &amiss_wire::model::ArtifactId,
        coordination: &amiss_wire::model::ArtifactId,
    ) -> Result<Option<(StoredStatus, String)>, RelationScheduleStoreError> {
        let _lock = self.lock()?;
        let metadata = self.load_metadata()?;
        let mut journal = self.open_committed_journal(&metadata)?;
        let mut state = self.state()?;
        synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
        match state.statuses.get(&(
            relation.as_str().to_owned(),
            coordination.as_str().to_owned(),
        )) {
            Some(StoredStatusState::Staged { status, .. }) => {
                let plan_binding = state
                    .relations
                    .get(&status.relation)
                    .map(|relation| relation.plan_binding.clone())
                    .ok_or(RelationScheduleStoreError::Corrupt)?;
                Ok(Some((status.as_ref().clone(), plan_binding)))
            }
            Some(StoredStatusState::Completed { .. }) | None => Ok(None),
        }
    }

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
        let mut metadata = self.load_metadata()?;
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
                ..
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
        self.append(
            &mut journal,
            &mut metadata,
            &mut state,
            JournalAction::Stage {
                plan_binding: checked.plan_binding,
                work_binding: checked.binding.work_binding,
                status: Box::new(stored),
            },
        )?;
        Ok(Some(record))
    }

    /// Claims the oldest unresolved status destination available through one
    /// operating-system lock shard. Dropping the returned value releases the
    /// claim without changing durable state.
    ///
    /// # Errors
    ///
    /// The journal, registry, retained audit, or destination binding cannot be
    /// trusted.
    pub fn claim_status_delivery(
        &self,
        registry: &RelationRegistry,
        artifacts: &FileArtifactStore,
    ) -> Result<Option<RelationStatusDeliveryClaim>, RelationScheduleStoreError> {
        let candidates = {
            let _lock = self.lock()?;
            let mut metadata = self.load_metadata()?;
            let mut journal = self.open_committed_journal(&metadata)?;
            let mut state = self.state()?;
            synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
            settle_acknowledged(self, &mut journal, &mut metadata, &mut state)?;
            pending_deliveries(&state)?
        };
        for candidate in candidates {
            let Some(delivery_lock) = self.try_delivery_lock(&candidate.destination)? else {
                continue;
            };
            let selected = {
                let _lock = self.lock()?;
                let mut metadata = self.load_metadata()?;
                let mut journal = self.open_committed_journal(&metadata)?;
                let mut state = self.state()?;
                synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
                settle_acknowledged(self, &mut journal, &mut metadata, &mut state)?;
                pending_deliveries(&state)?
                    .into_iter()
                    .find(|pending| pending.shard == candidate.shard)
            };
            let Some(selected) = selected else {
                continue;
            };
            let relation = amiss_wire::model::ArtifactId::new(selected.status.relation.clone())
                .ok_or(RelationScheduleStoreError::Corrupt)?;
            let coordination =
                amiss_wire::model::ArtifactId::new(selected.status.coordination.clone())
                    .ok_or(RelationScheduleStoreError::Corrupt)?;
            let (stored, plan_binding) = self
                .load_staged_status(&relation, &coordination)?
                .ok_or(RelationScheduleStoreError::Corrupt)?;
            if stored != selected.status {
                return Err(RelationScheduleStoreError::Corrupt);
            }
            let status = reopen_status(&stored, &plan_binding, registry, artifacts)?;
            let mut target = None;
            for candidate in &status.targets.destinations {
                if destination_binding(candidate)? == selected.destination
                    && target.replace(candidate.clone()).is_some()
                {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
            }
            return Ok(Some(RelationStatusDeliveryClaim {
                status,
                target: target.ok_or(RelationScheduleStoreError::Corrupt)?,
                status_binding: selected.status.status_binding,
                destination_binding: selected.destination,
                _delivery_lock: delivery_lock,
            }));
        }
        Ok(None)
    }

    /// Atomically records provider acceptance or reconciliation while the
    /// destination claim remains held. The final acknowledgement also commits
    /// batch completion.
    ///
    /// # Errors
    ///
    /// The claim was rebound or the durable state cannot be trusted.
    pub fn acknowledge_status_destination(
        &self,
        claim: RelationStatusDeliveryClaim,
    ) -> Result<RelationStatusRecord, RelationScheduleStoreError> {
        let RelationStatusDeliveryClaim {
            status,
            target,
            status_binding: claimed_status,
            destination_binding: claimed_destination,
            _delivery_lock: delivery_lock,
        } = claim;
        let stored = store_status(&status)?;
        let target_destination = destination_binding(&target)?;
        if stored.status_binding != claimed_status
            || target_destination != claimed_destination
            || !status.targets.destinations.contains(&target)
        {
            return Err(RelationScheduleStoreError::Status(
                RelationStatusError::BindingConflict,
            ));
        }
        let binding = claimed_status;
        let destination = claimed_destination;
        let key = (stored.relation.clone(), stored.coordination.clone());
        let _lock = self.lock()?;
        let mut metadata = self.load_metadata()?;
        let mut journal = self.open_committed_journal(&metadata)?;
        let mut state = self.state()?;
        synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
        match state.statuses.get(&key) {
            Some(StoredStatusState::Staged {
                binding: existing,
                status,
                acknowledged,
            }) if existing == &binding => {
                if status.as_ref() != &stored {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
                if acknowledged.contains(&destination)
                    || !stored.destinations.contains(&destination)
                {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
            }
            Some(_) | None => {
                return Err(RelationScheduleStoreError::Status(
                    RelationStatusError::BindingConflict,
                ));
            }
        }
        self.append(
            &mut journal,
            &mut metadata,
            &mut state,
            JournalAction::Acknowledge {
                relation: key.0.clone(),
                coordination: key.1.clone(),
                status_binding: binding.clone(),
                destination_binding: destination,
            },
        )?;
        let completed = matches!(
            state.statuses.get(&key),
            Some(StoredStatusState::Staged {
                status,
                acknowledged,
                ..
            }) if all_destinations_acknowledged(status, acknowledged)
        );
        let result = if completed {
            self.append(
                &mut journal,
                &mut metadata,
                &mut state,
                JournalAction::Complete {
                    relation: key.0,
                    coordination: key.1,
                    status_binding: binding,
                },
            )?;
            complete_relation_status(&status, &status)
                .map_err(RelationScheduleStoreError::Status)?
        } else {
            status
        };
        drop(delivery_lock);
        Ok(result)
    }

    /// Atomically marks the exact staged status batch complete. Repeating an
    /// acknowledged completion for the same immutable record is successful.
    /// Every configured destination must first have a durable acknowledgement.
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
        let mut metadata = self.load_metadata()?;
        let mut journal = self.open_committed_journal(&metadata)?;
        let mut state = self.state()?;
        synchronize(&mut state, &mut journal, &metadata, self.max_bindings)?;
        match state.statuses.get(&key) {
            Some(StoredStatusState::Staged {
                binding: existing,
                status,
                acknowledged,
            }) if existing == &binding => {
                if status.as_ref() != &stored {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
                if !all_destinations_acknowledged(&stored, acknowledged) {
                    return Err(RelationScheduleStoreError::DeliveryPending);
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
        self.append(
            &mut journal,
            &mut metadata,
            &mut state,
            JournalAction::Complete {
                relation: key.0,
                coordination: key.1,
                status_binding: binding,
            },
        )?;
        Ok(completed)
    }
}

fn settle_acknowledged(
    store: &FileRelationScheduleStore,
    journal: &mut File,
    metadata: &mut RootMetadata,
    state: &mut State,
) -> Result<(), RelationScheduleStoreError> {
    let completions = state
        .statuses
        .iter()
        .filter_map(|((relation, coordination), status)| match status {
            StoredStatusState::Staged {
                binding,
                status,
                acknowledged,
            } if all_destinations_acknowledged(status, acknowledged) => {
                Some(JournalAction::Complete {
                    relation: relation.clone(),
                    coordination: coordination.clone(),
                    status_binding: binding.clone(),
                })
            }
            StoredStatusState::Staged { .. } | StoredStatusState::Completed { .. } => None,
        })
        .collect::<Vec<_>>();
    for action in completions {
        store.append(journal, metadata, state, action)?;
    }
    Ok(())
}

fn all_destinations_acknowledged(status: &StoredStatus, acknowledged: &BTreeSet<String>) -> bool {
    status.destinations.len() == acknowledged.len()
        && status
            .destinations
            .iter()
            .all(|destination| acknowledged.contains(destination))
}

fn pending_deliveries(state: &State) -> Result<Vec<PendingDelivery>, RelationScheduleStoreError> {
    let mut destinations = BTreeMap::<&str, PendingDeliveryRef<'_>>::new();
    for status in state.statuses.values() {
        let StoredStatusState::Staged {
            status,
            acknowledged,
            ..
        } = status
        else {
            continue;
        };
        if !state.relations.contains_key(&status.relation) {
            return Err(RelationScheduleStoreError::Corrupt);
        }
        for destination in &status.destinations {
            if acknowledged.contains(destination) {
                continue;
            }
            let digest =
                Digest::from_wire(destination).ok_or(RelationScheduleStoreError::Corrupt)?;
            let pending = PendingDeliveryRef {
                status,
                destination,
                shard: digest.as_bytes()[0],
            };
            match destinations.get(destination.as_str()) {
                Some(existing) if existing.status.relation != status.relation => {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
                Some(existing) if existing.status.fence == status.fence => {
                    return Err(RelationScheduleStoreError::Corrupt);
                }
                Some(existing) if existing.status.fence < status.fence => {}
                Some(_) | None => {
                    destinations.insert(destination, pending);
                }
            }
        }
    }
    let mut pending = destinations.into_values().collect::<Vec<_>>();
    pending.sort_by(|left, right| {
        left.status
            .fence
            .cmp(&right.status.fence)
            .then_with(|| left.status.relation.cmp(&right.status.relation))
            .then_with(|| left.status.coordination.cmp(&right.status.coordination))
            .then_with(|| left.destination.cmp(right.destination))
    });
    let mut shards = BTreeSet::new();
    pending.retain(|delivery| shards.insert(delivery.shard));
    Ok(pending
        .into_iter()
        .map(|delivery| PendingDelivery {
            status: delivery.status.clone(),
            destination: delivery.destination.to_owned(),
            shard: delivery.shard,
        })
        .collect())
}
