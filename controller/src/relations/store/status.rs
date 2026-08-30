use amiss_wire::digest::{Digest, hb};
use serde::{Deserialize, Serialize};

use super::RelationScheduleStoreError;
use crate::artifacts::valid_artifact_id;
use crate::{ArtifactAuditDigests, RelationStatusRecord, RelationStatusTarget};

const STATUS_BINDING_DOMAIN: &str = "amiss/controller-relation-status-binding-v1";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredStatus {
    pub(super) relation: String,
    pub(super) coordination: String,
    pub(super) trigger_role: String,
    pub(super) fence: u64,
    pub(super) status_binding: String,
    artifact_id: String,
}

#[derive(Serialize)]
struct BoundStatus<'a> {
    relation: &'a str,
    coordination: &'a str,
    trigger_role: &'a str,
    fence: u64,
    destinations: Vec<BoundDestination<'a>>,
    artifact: BoundArtifact<'a>,
    audit: BoundAudit<'a>,
}

#[derive(Serialize)]
struct BoundDestination<'a> {
    role: &'a str,
    provider_namespace: &'a str,
    provider_instance: &'a str,
    integration: &'a str,
    repository_host: &'a str,
    repository_owner: &'a str,
    repository_name: &'a str,
    credential: &'a str,
    object_format: &'a str,
    candidate_commit: &'a str,
    required_status_name: &'a str,
}

#[derive(Serialize)]
struct BoundArtifact<'a> {
    id: &'a str,
    locator: &'a str,
    expires_at_unix_millis: i64,
    report_digest: &'a [u8; 32],
}

#[derive(Serialize)]
struct BoundAudit<'a> {
    report_digest: &'a [u8; 32],
    plan_digest: &'a [u8; 32],
    evidence_digest: Option<&'a [u8; 32]>,
    assessment_digest: &'a [u8; 32],
    verdict: &'a str,
}

pub(super) fn store_status(
    record: &RelationStatusRecord,
) -> Result<StoredStatus, RelationScheduleStoreError> {
    if record.completed {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    let stored = StoredStatus {
        relation: record.targets.relation.as_str().to_owned(),
        coordination: record.targets.coordination.as_str().to_owned(),
        trigger_role: record.targets.trigger_role.as_str().to_owned(),
        fence: record.targets.fence.get(),
        status_binding: record_binding(record)?,
        artifact_id: record.audit.artifact.id.clone(),
    };
    validate_stored_status(&stored)?;
    Ok(stored)
}

pub(super) fn validate_stored_status(
    stored: &StoredStatus,
) -> Result<(), RelationScheduleStoreError> {
    if amiss_wire::model::ArtifactId::new(stored.relation.clone()).is_none()
        || amiss_wire::model::ArtifactId::new(stored.coordination.clone()).is_none()
        || amiss_wire::model::ArtifactId::new(stored.trigger_role.clone()).is_none()
        || crate::LeaseFence::new(stored.fence).is_none()
        || Digest::from_wire(&stored.status_binding).is_none()
        || !valid_artifact_id(&stored.artifact_id)
    {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    Ok(())
}

fn record_binding(record: &RelationStatusRecord) -> Result<String, RelationScheduleStoreError> {
    let ArtifactAuditDigests::Relation(audit) = record.audit.audit else {
        return Err(RelationScheduleStoreError::Corrupt);
    };
    let destinations = record
        .targets
        .destinations
        .iter()
        .map(bound_destination)
        .collect();
    let bytes = serde_json::to_vec(&BoundStatus {
        relation: record.targets.relation.as_str(),
        coordination: record.targets.coordination.as_str(),
        trigger_role: record.targets.trigger_role.as_str(),
        fence: record.targets.fence.get(),
        destinations,
        artifact: BoundArtifact {
            id: &record.audit.artifact.id,
            locator: &record.audit.artifact.locator,
            expires_at_unix_millis: record.audit.artifact.expires_at_unix_millis,
            report_digest: record.audit.artifact.report_digest.as_bytes(),
        },
        audit: BoundAudit {
            report_digest: audit.report_digest.as_bytes(),
            plan_digest: audit.plan_digest.as_bytes(),
            evidence_digest: audit.evidence_digest.as_ref().map(Digest::as_bytes),
            assessment_digest: audit.assessment_digest.as_bytes(),
            verdict: audit.verdict.as_ref(),
        },
    })
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    Ok(hb(STATUS_BINDING_DOMAIN, &bytes).to_string())
}

fn bound_destination(target: &RelationStatusTarget) -> BoundDestination<'_> {
    BoundDestination {
        role: target.role.as_str(),
        provider_namespace: target.scope.provider.namespace.as_str(),
        provider_instance: target.scope.provider.instance.as_str(),
        integration: target.scope.integration.as_str(),
        repository_host: target.scope.repository.host(),
        repository_owner: target.scope.repository.owner(),
        repository_name: target.scope.repository.name(),
        credential: target.credential.as_str(),
        object_format: target.candidate_commit.object_format().into(),
        candidate_commit: target.candidate_commit.as_str(),
        required_status_name: &target.required_status_name,
    }
}
