use amiss_controller::{
    ArtifactAuditBundle, ArtifactError, AuthenticatedDelivery, ControllerEvaluationId,
    FileArtifactStore, FileRelationScheduleStore, PendingRelation, ProviderError,
    RelationAcquiredRoot, RelationAcquisitionError, RelationAuditBundle, RelationCredentialError,
    RelationCredentialRouter, RelationLookupError, RelationRegistry, RelationScheduleStoreError,
    RelationStatusRecord, RelationStatusTarget, RelationSubjectHead, RelationSubjectTransition,
    RelationTransition, TriggeredRelation, relation_audit_plan, relation_authority,
    relation_transition, relations_for_delivery,
};
use amiss_controller_git::{
    RelationProjectionError, RelationProjectionRequest, project_relation_evidence,
};
use amiss_wire::digest::Digest;
use amiss_wire::json;
use amiss_wire::model::ArtifactId;
use amiss_wire::relation::{assess, parse_evidence, parse_plan};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatedRelation {
    pub delivery: AuthenticatedDelivery,
    pub relation: TriggeredRelation,
    pub coordination: ArtifactId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatedTransition {
    pub delivery: AuthenticatedDelivery,
    pub transition: RelationTransition,
}

pub struct RelationAuditRequest<'a> {
    pub evaluation_id: &'a ControllerEvaluationId,
    pub pending: &'a PendingRelation,
    pub report: &'a [u8],
    pub roots: [RelationAcquiredRoot<'a>; 2],
    pub heads: [RelationSubjectHead; 2],
    pub engine_version: &'a str,
    pub engine_digest: Digest,
}

#[derive(Debug, thiserror::Error)]
pub enum RelationAuditExecutionError {
    #[error("relation work was superseded before evaluation")]
    Superseded,
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error(transparent)]
    Projection(#[from] RelationProjectionError),
    #[error(transparent)]
    Wire(#[from] amiss_wire::de::Error),
    #[error(transparent)]
    Schedule(#[from] RelationScheduleStoreError),
}

#[derive(Debug, thiserror::Error)]
pub enum RelationOutboxError {
    #[error(transparent)]
    Credential(#[from] RelationCredentialError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Schedule(#[from] RelationScheduleStoreError),
}

/// Binds one operator-declared coordination identity to a relation owned by an authenticated
/// delivery. The identity stays opaque; this boundary derives nothing from revisions or time.
///
/// # Errors
///
/// The delivery is internally inconsistent or does not own the declared relation.
pub fn admit_relation_coordination(
    registry: &RelationRegistry,
    delivery: AuthenticatedDelivery,
    relation: &ArtifactId,
    coordination: ArtifactId,
) -> Result<CoordinatedRelation, RelationLookupError> {
    let relation = relations_for_delivery(registry, &delivery)?
        .into_iter()
        .find(|triggered| triggered.plan.identity == *relation)
        .ok_or(RelationLookupError)?;
    Ok(CoordinatedRelation {
        delivery,
        relation,
        coordination,
    })
}

/// Freezes coordinator-resolved subject revisions and keeps them bound to the authenticated
/// delivery that selected the relation.
///
/// # Errors
///
/// The live registry does not admit the relation and delivery, the revisions do not reproduce that
/// relation, or the trigger candidate differs from the delivery-authenticated candidate.
pub fn freeze_relation_transition(
    registry: &RelationRegistry,
    coordinated: CoordinatedRelation,
    subjects: [RelationSubjectTransition; 2],
) -> Result<CoordinatedTransition, RelationAcquisitionError> {
    let identity = coordinated.relation.plan.identity.clone();
    let CoordinatedRelation {
        delivery,
        relation,
        coordination,
    } = admit_relation_coordination(
        registry,
        coordinated.delivery,
        &identity,
        coordinated.coordination,
    )
    .map_err(|_defect| RelationAcquisitionError::InvalidTransition)?;
    let transition = relation_transition(relation, coordination, subjects)?;
    transition
        .subjects
        .iter()
        .any(|subject| {
            subject.role == transition.relation.trigger_role
                && subject.commits.candidate == delivery.provider_run.candidate_commit
        })
        .then_some(CoordinatedTransition {
            delivery,
            transition,
        })
        .ok_or(RelationAcquisitionError::InvalidTransition)
}

/// Projects, assesses, retains, and durably stages one current relation audit.
///
/// The early fence check avoids spending projection work on an already superseded transition. The
/// durable stage rechecks the same fence under the scheduling lock before exposing destinations.
///
/// # Errors
///
/// The pending transition is stale, the report or acquired roots cannot reproduce it, assessment
/// construction fails, immutable artifact retention fails, or status staging cannot commit.
pub fn execute_relation_audit(
    artifacts: &FileArtifactStore,
    schedules: &FileRelationScheduleStore,
    request: RelationAuditRequest<'_>,
) -> Result<Option<RelationStatusRecord>, RelationAuditExecutionError> {
    schedules
        .is_current(request.pending)?
        .then_some(())
        .ok_or(RelationAuditExecutionError::Superseded)?;
    let plan_bytes = relation_audit_plan(&request.pending.transition, request.report)?;
    let plan = parse_plan(&plan_bytes)?;
    let evidence_bytes = json::canonical(&project_relation_evidence(RelationProjectionRequest {
        transition: &request.pending.transition,
        plan: &plan,
        roots: request.roots,
    })?);
    let evidence = parse_evidence(&evidence_bytes)?;
    let assessment_bytes = json::canonical(&assess(
        &plan,
        Some(&evidence),
        request.engine_version,
        request.engine_digest,
    )?);
    let bundle = RelationAuditBundle {
        transition: &request.pending.transition,
        report: request.report,
        plan: &plan_bytes,
        evidence: Some(&evidence_bytes),
        assessment: &assessment_bytes,
    };
    let audit =
        artifacts.retain_audit(request.evaluation_id, ArtifactAuditBundle::Relation(bundle))?;
    Ok(schedules.stage_status(artifacts, request.pending, request.heads, audit, bundle)?)
}

/// Delivers every currently claimable relation status through its frozen credential authority.
///
/// The publisher must return success only after the provider accepts or reconciles the exact
/// status and target. A failed route or publication drops the claim without acknowledging it.
///
/// # Errors
///
/// A credential route, provider publication, or durable schedule transition fails.
pub fn drain_relation_outbox<A>(
    registry: &RelationRegistry,
    credentials: &RelationCredentialRouter<A>,
    artifacts: &FileArtifactStore,
    schedules: &FileRelationScheduleStore,
    mut publish: impl FnMut(
        &A,
        &RelationStatusRecord,
        &RelationStatusTarget,
    ) -> Result<(), ProviderError>,
) -> Result<(), RelationOutboxError> {
    while let Some(claim) = schedules.claim_status_delivery(registry, artifacts)? {
        let authority =
            relation_authority(credentials, &claim.target.credential, &claim.target.scope)?;
        publish(authority, &claim.status, &claim.target)?;
        schedules.acknowledge_status_destination(claim)?;
    }
    Ok(())
}
