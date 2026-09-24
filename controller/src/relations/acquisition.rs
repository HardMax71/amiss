use std::path::Path;

use amiss_wire::model::ArtifactId;

use crate::acquisition::verify_commits;
use crate::{RelationAcquisitionError, RelationTransition, relation_transition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationAcquiredRoot<'a> {
    pub role: &'a ArtifactId,
    pub repository: &'a Path,
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
    let checked = relation_transition(
        transition.relation.clone(),
        transition.coordination.clone(),
        transition.subjects.clone(),
    )?;
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
