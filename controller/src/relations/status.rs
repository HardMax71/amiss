use amiss_wire::model::{ArtifactId, Oid};

use crate::LeaseFence;

use super::{PendingRelation, RelationSubject, relation_transition};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSubjectHead {
    pub subject: RelationSubject,
    pub candidate_commit: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationStatusTarget {
    pub subject: RelationSubject,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationStatusError {
    #[error("the pending relation transition is invalid")]
    InvalidTransition,
    #[error("the finality facts do not exactly name both relation subjects")]
    InvalidHeads,
    #[error("a selected relation subject head was superseded")]
    Superseded,
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
                subject: subject.clone(),
                candidate_commit: frozen.commits.candidate.clone(),
                required_status_name: destination.required_status_name.clone(),
            })
        })
        .collect::<Result<Vec<_>, RelationStatusError>>()?;
    destinations.sort_by(|left, right| left.subject.role.cmp(&right.subject.role));
    Ok(RelationStatusTargets {
        relation: plan.identity.clone(),
        coordination: transition.coordination,
        trigger_role: transition.relation.trigger_role,
        fence: pending.fence,
        destinations,
    })
}
