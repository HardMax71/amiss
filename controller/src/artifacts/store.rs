mod disk;

use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::format::{Blob, PublicationAudit, Record, RecordInput, Root};
use super::{
    ArtifactBundle, ArtifactCleanup, ArtifactComponent, ArtifactError, ArtifactReference,
    ArtifactStoreConfig, PublicationAuditReference,
};
use crate::{ControllerClock, ControllerEvaluationId, PublicationAuditBundle};

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
        let payloads = [
            (ArtifactComponent::Report, Some(bundle.report)),
            (ArtifactComponent::Semantic, bundle.semantic),
            (ArtifactComponent::Plan, bundle.plan),
            (ArtifactComponent::Evidence, bundle.evidence),
            (ArtifactComponent::Assessment, bundle.assessment),
        ];
        self.retain_record(evaluation_id, input, payloads)
    }

    /// Retains one validated publication audit as exact immutable bytes.
    ///
    /// # Errors
    ///
    /// The audit is invalid, the evaluation was rebound, capacity is exhausted,
    /// or durable storage cannot be trusted.
    pub fn retain_publication_audit(
        &self,
        evaluation_id: &ControllerEvaluationId,
        bundle: PublicationAuditBundle<'_>,
    ) -> Result<PublicationAuditReference, ArtifactError> {
        let digests = crate::validate_publication_audit(bundle)?;
        let input = RecordInput {
            report: Blob::from_digest(bundle.report, digests.report_digest)?,
            semantic: None,
            plan: None,
            evidence: None,
            assessment: None,
            external_tally: None,
            external_incomplete: false,
            publication_audit: Some(PublicationAudit {
                plan: Blob::from_digest(bundle.plan, digests.plan_digest)?,
                evidence: bundle
                    .evidence
                    .zip(digests.evidence_digest)
                    .map(|(bytes, digest)| Blob::from_digest(bytes, digest))
                    .transpose()?,
                assessment: Blob::from_digest(bundle.assessment, digests.assessment_digest)?,
                verdict: digests.verdict.as_ref().to_owned(),
            }),
        };
        let payloads = [
            (ArtifactComponent::Report, Some(bundle.report)),
            (ArtifactComponent::PublicationPlan, Some(bundle.plan)),
            (ArtifactComponent::PublicationEvidence, bundle.evidence),
            (
                ArtifactComponent::PublicationAssessment,
                Some(bundle.assessment),
            ),
        ];
        let artifact = self.retain_record(evaluation_id, input, payloads)?;
        Ok(PublicationAuditReference {
            artifact,
            audit: digests,
        })
    }

    fn retain_record<const N: usize>(
        &self,
        evaluation_id: &ControllerEvaluationId,
        input: RecordInput,
        payloads: [(ArtifactComponent, Option<&[u8]>); N],
    ) -> Result<ArtifactReference, ArtifactError> {
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
        let metadata_bytes = super::format::encode_record(&record)?;
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
        if let Err(error) = disk::write_record(&self.root, &record, &metadata_bytes, payloads) {
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
            disk::read_blob(
                &disk::component_path(&self.root, &stored.metadata.id, component),
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
        if !super::format::valid_id(id) {
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
        disk::read_blob(&disk::component_path(&self.root, id, component), blob)
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
        Ok(disk::trusted_now(self.clock.as_ref())?.max(state.root.clock_high_water_unix_millis))
    }

    fn advance_clock(&self, state: &mut State, now: i64) -> Result<(), ArtifactError> {
        if now == state.root.clock_high_water_unix_millis {
            return Ok(());
        }
        state.root.clock_high_water_unix_millis = now;
        if let Err(error) = disk::save_root(&self.root, &state.root) {
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
            if let Err(error) = disk::remove_record(&self.root, &stored.metadata) {
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

fn require_trusted(state: &State) -> Result<(), ArtifactError> {
    state.trusted.then_some(()).ok_or(ArtifactError::Corrupt)
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
    if let Some(semantic) = bundle.semantic {
        super::semantic::validate(bundle.report, semantic)?;
    }
    Ok(RecordInput {
        report: Blob::new(bundle.report)?,
        semantic: bundle.semantic.map(Blob::new).transpose()?,
        plan: bundle.plan.map(Blob::new).transpose()?,
        evidence: bundle.evidence.map(Blob::new).transpose()?,
        assessment: bundle.assessment.map(Blob::new).transpose()?,
        external_tally: bundle.external_tally,
        external_incomplete: bundle.external_incomplete,
        publication_audit: None,
    })
}
