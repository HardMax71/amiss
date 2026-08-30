use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::ArtifactId;
use amiss_wire::relation::parse_plan;
use serde::{Deserialize, Serialize};

use super::RelationScheduleStoreError;
use super::binding::plan_binding;
use crate::artifacts::valid_artifact_id;
use crate::{
    ArtifactAuditDigests, ArtifactAuditReference, FileArtifactStore, LeaseFence, OidPair,
    PendingRelation, RelationAuditBundle, RelationRegistry, RelationScheduleError,
    RelationStatusRecord, RelationStatusTarget, RelationSubjectHead, RelationSubjectTransition,
    TriggeredRelation, relation_transition, stage_relation_status, validate_relation_audit,
};

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
    if ArtifactId::new(stored.relation.clone()).is_none()
        || ArtifactId::new(stored.coordination.clone()).is_none()
        || ArtifactId::new(stored.trigger_role.clone()).is_none()
        || LeaseFence::new(stored.fence).is_none()
        || Digest::from_wire(&stored.status_binding).is_none()
        || !valid_artifact_id(&stored.artifact_id)
    {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    Ok(())
}

pub(super) fn reopen_status(
    stored: &StoredStatus,
    committed_plan_binding: &str,
    registry: &RelationRegistry,
    artifacts: &FileArtifactStore,
) -> Result<RelationStatusRecord, RelationScheduleStoreError> {
    validate_stored_status(stored)?;
    let relation =
        ArtifactId::new(stored.relation.clone()).ok_or(RelationScheduleStoreError::Corrupt)?;
    let plan = registry
        .plans
        .get(&relation)
        .ok_or(RelationScheduleStoreError::Configuration)?;
    if plan_binding(plan.as_ref())?.to_string() != committed_plan_binding {
        return Err(RelationScheduleStoreError::Schedule(
            RelationScheduleError::BindingConflict,
        ));
    }
    let retained = artifacts
        .reopen_relation_audit(&stored.artifact_id)
        .map_err(RelationScheduleStoreError::Artifact)?;
    let parsed =
        parse_plan(&retained.plan).map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    let trigger_role =
        ArtifactId::new(stored.trigger_role.clone()).ok_or(RelationScheduleStoreError::Corrupt)?;
    if parsed.payload.relation.identity != relation
        || parsed.payload.coordination.as_str() != stored.coordination
        || parsed.payload.trigger_role != trigger_role
    {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    let subjects = parsed
        .payload
        .subjects
        .each_ref()
        .map(|subject| RelationSubjectTransition {
            role: subject.role.clone(),
            commits: OidPair {
                base: subject.base.commit.clone(),
                candidate: subject.candidate.commit.clone(),
            },
            trees: OidPair {
                base: subject.base.tree.clone(),
                candidate: subject.candidate.tree.clone(),
            },
        });
    let transition = relation_transition(
        TriggeredRelation {
            plan: std::sync::Arc::clone(plan),
            trigger_role,
        },
        parsed.payload.coordination,
        subjects,
    )
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    let pending = PendingRelation {
        transition,
        fence: LeaseFence::new(stored.fence).ok_or(RelationScheduleStoreError::Corrupt)?,
    };
    let heads = pending.transition.subjects.each_ref().map(|frozen| {
        pending
            .transition
            .relation
            .plan
            .subjects
            .iter()
            .find(|subject| subject.role == frozen.role)
            .map(|subject| RelationSubjectHead {
                subject: subject.clone(),
                candidate_commit: frozen.commits.candidate.clone(),
            })
            .ok_or(RelationScheduleStoreError::Corrupt)
    });
    let [documentation, source] = heads;
    let heads = [documentation?, source?];
    let bundle = RelationAuditBundle {
        transition: &pending.transition,
        report: &retained.report,
        plan: &retained.plan,
        evidence: retained.evidence.as_deref(),
        assessment: &retained.assessment,
    };
    let audit =
        validate_relation_audit(bundle).map_err(|_defect| RelationScheduleStoreError::Corrupt)?;
    let record = stage_relation_status(
        &pending,
        Some(&pending),
        heads,
        None,
        ArtifactAuditReference {
            artifact: retained.artifact,
            audit: ArtifactAuditDigests::Relation(audit),
        },
        bundle,
    )
    .map_err(|_defect| RelationScheduleStoreError::Corrupt)?
    .ok_or(RelationScheduleStoreError::Corrupt)?;
    if store_status(&record)? != *stored {
        return Err(RelationScheduleStoreError::Corrupt);
    }
    Ok(record)
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
