use std::path::Path;

use amiss_wire::model::ArtifactId;

use crate::acquisition::verify_commits;
use crate::{OidPair, TriggeredRelation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSubjectTransition {
    pub role: ArtifactId,
    pub commits: OidPair,
    pub trees: OidPair,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationTransition {
    pub relation: TriggeredRelation,
    pub subjects: [RelationSubjectTransition; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationAcquiredRoot<'a> {
    pub role: &'a ArtifactId,
    pub repository: &'a Path,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationAcquisitionError {
    #[error("the frozen relation transition is inconsistent with its operator plan")]
    InvalidTransition,
    #[error("one or more exact relation subjects cannot be proven")]
    Unproven,
}

/// Freezes two independently resolved base/candidate pairs against one
/// operator-owned relation selected by an authenticated trigger.
///
/// # Errors
///
/// The relation, trigger role, subject roles, or object formats do not exactly
/// reproduce the registered plan.
pub fn relation_transition(
    relation: TriggeredRelation,
    mut subjects: [RelationSubjectTransition; 2],
) -> Result<RelationTransition, RelationAcquisitionError> {
    subjects.sort_by(|left, right| left.role.cmp(&right.role));
    let plan = relation.plan.as_ref();
    let relation_valid = super::validate_relation(plan).is_ok()
        && plan
            .subjects
            .iter()
            .any(|subject| subject.role == relation.trigger_role);
    let subjects_valid = subjects[0].role != subjects[1].role
        && subjects.iter().all(|transition| {
            plan.subjects
                .iter()
                .find(|subject| subject.role == transition.role)
                .is_some_and(|subject| {
                    [
                        &transition.commits.base,
                        &transition.commits.candidate,
                        &transition.trees.base,
                        &transition.trees.candidate,
                    ]
                    .into_iter()
                    .all(|oid| oid.object_format() == subject.object_format)
                })
        });
    if !relation_valid || !subjects_valid {
        return Err(RelationAcquisitionError::InvalidTransition);
    }
    Ok(RelationTransition { relation, subjects })
}

/// Rechecks the frozen transition and proves every acquired commit names its
/// independently resolved tree in a distinct repository object store.
///
/// # Errors
///
/// The transition changed, roots were substituted or aliased, an object is
/// missing or malformed, or a commit names a different tree.
pub fn verify_relation_acquired<'a>(
    transition: &RelationTransition,
    mut roots: [RelationAcquiredRoot<'a>; 2],
) -> Result<[RelationAcquiredRoot<'a>; 2], RelationAcquisitionError> {
    let checked = relation_transition(transition.relation.clone(), transition.subjects.clone())?;
    roots.sort_by(|left, right| left.role.cmp(right.role));
    if !same_file::is_same_file(roots[0].repository, roots[1].repository).is_ok_and(|same| !same)
        || roots
            .iter()
            .zip(&checked.subjects)
            .any(|(root, subject)| root.role != &subject.role)
    {
        return Err(RelationAcquisitionError::Unproven);
    }

    roots
        .iter()
        .zip(&checked.subjects)
        .try_for_each(|(root, transition)| {
            let subject = checked
                .relation
                .plan
                .subjects
                .iter()
                .find(|subject| subject.role == transition.role)
                .ok_or(RelationAcquisitionError::InvalidTransition)?;
            verify_commits(
                root.repository,
                subject.object_format,
                [
                    (&transition.commits.base, &transition.trees.base),
                    (&transition.commits.candidate, &transition.trees.candidate),
                ],
                RelationAcquisitionError::Unproven,
                RelationAcquisitionError::Unproven,
            )
        })?;
    Ok(roots)
}
