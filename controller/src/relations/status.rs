use amiss_wire::controls::valid_required_status_name;
use amiss_wire::model::{ArtifactId, Oid};
use amiss_wire::relation::RelationVerdict;

use crate::artifacts::checked_reference;
use crate::{
    ArtifactAuditDigests, ArtifactAuditReference, LeaseFence, OpaqueId, PlanScope,
    RelationAuditBundle, validate_relation_audit,
};

use super::{PendingRelation, RelationSubject, relation_transition};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSubjectHead {
    pub subject: RelationSubject,
    pub candidate_commit: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationStatusTarget {
    pub role: ArtifactId,
    pub scope: PlanScope,
    pub credential: OpaqueId,
    pub candidate_commit: Oid,
    pub required_status_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationStatusTargets {
    pub relation: ArtifactId,
    pub coordination: ArtifactId,
    pub trigger_role: ArtifactId,
    pub fence: LeaseFence,
    pub destinations: Vec<RelationStatusTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationStatusRecord {
    pub targets: RelationStatusTargets,
    pub audit: ArtifactAuditReference,
    pub completed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationStatusPublication {
    pub summary: String,
    pub passing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationStatusError {
    #[error("the pending relation transition is invalid")]
    InvalidTransition,
    #[error("the finality facts do not exactly name both relation subjects")]
    InvalidHeads,
    #[error("a selected relation subject head was superseded")]
    Superseded,
    #[error("the relation audit does not bind the pending transition and retained artifact")]
    InvalidAudit,
    #[error("the relation status identity is already bound to different immutable data")]
    BindingConflict,
    #[error("the relation status record is not one exact unfinished publication")]
    InvalidPublication,
}

/// Projects one staged destination into credential-free provider output.
///
/// # Errors
///
/// The record is completed, malformed, does not contain the target exactly
/// once, or its retained artifact does not reproduce the relation audit.
pub fn relation_status_publication(
    status: &RelationStatusRecord,
    target: &RelationStatusTarget,
) -> Result<RelationStatusPublication, RelationStatusError> {
    let destinations = &status.targets.destinations;
    let exact_target = destinations.iter().filter(|item| *item == target).count() == 1;
    let ordered_targets = (1..=2).contains(&destinations.len())
        && destinations
            .windows(2)
            .all(|pair| matches!(pair, [left, right] if left.role < right.role));
    let ArtifactAuditDigests::Relation(audit) = status.audit.audit else {
        return Err(RelationStatusError::InvalidPublication);
    };
    let artifact = &status.audit.artifact;
    if status.completed
        || !exact_target
        || !ordered_targets
        || !valid_required_status_name(&target.required_status_name)
        || artifact.report_digest != audit.report_digest
        || artifact.semantic_digest.is_some()
        || artifact.assessment_digest.is_some()
        || artifact.external_tally.is_some()
        || artifact.external_incomplete
        || (audit.verdict != RelationVerdict::Unproven && audit.evidence_digest.is_none())
    {
        return Err(RelationStatusError::InvalidPublication);
    }

    let evidence = audit
        .evidence_digest
        .map_or_else(|| "none".to_owned(), |digest| digest.to_string());
    let repository = &target.scope.repository;
    Ok(RelationStatusPublication {
        summary: format!(
            "relation: {}\ncoordination: {}\nfence: {}\nverdict: {}\nprovider: {}/{}\nrepository: {}/{}/{}\ntrigger-role: {}\ndestination-role: {}\nstatus: {}\ncandidate-commit: {}\nreport: {}\nplan: {}\nevidence: {evidence}\nassessment: {}",
            status.targets.relation.as_str(),
            status.targets.coordination.as_str(),
            status.targets.fence.get(),
            audit.verdict.as_ref(),
            target.scope.provider.namespace,
            target.scope.provider.instance,
            repository.host(),
            repository.owner(),
            repository.name(),
            status.targets.trigger_role.as_str(),
            target.role.as_str(),
            target.required_status_name,
            target.candidate_commit.as_str(),
            audit.report_digest,
            audit.plan_digest,
            audit.assessment_digest,
        ),
        passing: matches!(
            audit.verdict,
            RelationVerdict::Aligned | RelationVerdict::ResolvedDrift
        ),
    })
}

/// Rechecks both independently refreshed subject heads and freezes only the
/// operator-configured status destinations under the pending fence.
///
/// This is a pure preparation step. A durable publisher must still stage the
/// returned value while proving that the fence remains current.
///
/// # Errors
///
/// The pending transition or finality facts are inconsistent, or either
/// selected candidate commit is no longer current.
pub fn relation_status_targets(
    pending: &PendingRelation,
    mut heads: [RelationSubjectHead; 2],
) -> Result<RelationStatusTargets, RelationStatusError> {
    let transition = relation_transition(
        pending.transition.relation.clone(),
        pending.transition.coordination.clone(),
        pending.transition.subjects.clone(),
    )
    .map_err(|_defect| RelationStatusError::InvalidTransition)?;
    heads.sort_by(|left, right| left.subject.role.cmp(&right.subject.role));
    let plan = transition.relation.plan.as_ref();
    if heads
        .iter()
        .zip(&transition.subjects)
        .any(|(head, frozen)| {
            head.subject.role != frozen.role
                || head.candidate_commit.object_format() != head.subject.object_format
                || plan
                    .subjects
                    .iter()
                    .find(|subject| subject.role == frozen.role)
                    != Some(&head.subject)
        })
    {
        return Err(RelationStatusError::InvalidHeads);
    }
    if heads
        .iter()
        .zip(&transition.subjects)
        .any(|(head, frozen)| head.candidate_commit != frozen.commits.candidate)
    {
        return Err(RelationStatusError::Superseded);
    }

    let mut destinations = plan
        .status_destinations
        .iter()
        .map(|destination| {
            let subject = plan
                .subjects
                .iter()
                .find(|subject| subject.role == destination.subject_role)
                .ok_or(RelationStatusError::InvalidTransition)?;
            let frozen = transition
                .subjects
                .iter()
                .find(|frozen| frozen.role == destination.subject_role)
                .ok_or(RelationStatusError::InvalidTransition)?;
            Ok(RelationStatusTarget {
                role: subject.role.clone(),
                scope: subject.scope.clone(),
                credential: subject.credential.clone(),
                candidate_commit: frozen.commits.candidate.clone(),
                required_status_name: destination.required_status_name.clone(),
            })
        })
        .collect::<Result<Vec<_>, RelationStatusError>>()?;
    destinations.sort_by(|left, right| left.role.cmp(&right.role));
    Ok(RelationStatusTargets {
        relation: plan.identity.clone(),
        coordination: transition.coordination,
        trigger_role: transition.relation.trigger_role,
        fence: pending.fence,
        destinations,
    })
}

/// Freezes one exact retained relation result before provider I/O. An exact
/// unfinished retry returns the original record, while an exact completed
/// retry returns `None`.
///
/// The current fence and independently refreshed heads are inputs because an
/// artifact reference is not external-write authorization by itself.
///
/// # Errors
///
/// The pending fence is stale, finality changed, the full audit cannot be
/// replayed against the pending transition and reference, or the same status
/// identity was already staged with different immutable data.
pub fn stage_relation_status(
    pending: &PendingRelation,
    current: Option<&PendingRelation>,
    heads: [RelationSubjectHead; 2],
    previous: Option<&RelationStatusRecord>,
    audit: ArtifactAuditReference,
    bundle: RelationAuditBundle<'_>,
) -> Result<Option<RelationStatusRecord>, RelationStatusError> {
    if current != Some(pending) {
        return Err(RelationStatusError::Superseded);
    }
    let targets = relation_status_targets(pending, heads)?;
    if bundle.transition != &pending.transition {
        return Err(RelationStatusError::InvalidAudit);
    }
    let digests =
        validate_relation_audit(bundle).map_err(|_defect| RelationStatusError::InvalidAudit)?;
    if audit.audit != ArtifactAuditDigests::Relation(digests)
        || checked_reference(audit.artifact.clone()).is_none()
        || audit.artifact.report_digest != digests.report_digest
        || audit.artifact.semantic_digest.is_some()
        || audit.artifact.assessment_digest.is_some()
        || audit.artifact.external_tally.is_some()
        || audit.artifact.external_incomplete
    {
        return Err(RelationStatusError::InvalidAudit);
    }
    let requested = RelationStatusRecord {
        targets,
        audit,
        completed: false,
    };
    let Some(previous) = previous else {
        return Ok(Some(requested));
    };
    if previous.targets != requested.targets || previous.audit != requested.audit {
        return Err(RelationStatusError::BindingConflict);
    }
    Ok((!previous.completed).then(|| previous.clone()))
}

/// Completes only the exact staged record and preserves an earlier successful
/// completion across an ambiguous retry.
///
/// # Errors
///
/// The durable current record differs from the staged value supplied by the
/// publisher.
pub fn complete_relation_status(
    current: &RelationStatusRecord,
    staged: &RelationStatusRecord,
) -> Result<RelationStatusRecord, RelationStatusError> {
    if current.targets != staged.targets || current.audit != staged.audit {
        return Err(RelationStatusError::BindingConflict);
    }
    let mut completed = current.clone();
    completed.completed = true;
    Ok(completed)
}
