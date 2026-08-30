use amiss_wire::model::{ArtifactId, Oid};

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
