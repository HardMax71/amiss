mod projection;
mod tests;

use std::path::Path;
use std::sync::atomic::AtomicBool;

use amiss_controller::{
    OpaqueId, RelationAcquiredRoot, RelationAcquisitionError, RelationLimits, RelationTransition,
    relation_transition, verify_relation_acquired,
};
use amiss_wire::model::{ArtifactId, ObjectFormat};

use crate::{
    ExactFetch, ExactWant, GitCredential, GitFetchBounds, GitFetchLimits, GitFetchUsage,
    REPOSITORY_CANDIDATE_REF, REPOSITORY_TARGET_REF, fetch_exact,
};

pub use projection::{
    RelationProjectionError, RelationProjectionRequest, project_relation_evidence,
};

#[derive(Clone, Copy)]
pub struct RelationGitSubject<'a> {
    pub role: &'a ArtifactId,
    pub credential_id: &'a OpaqueId,
    pub url: &'a str,
    pub credential: GitCredential<'a>,
    pub destination: &'a Path,
}

#[derive(Clone, Copy)]
pub struct RelationGitFetch<'a> {
    pub transition: &'a RelationTransition,
    pub subjects: [RelationGitSubject<'a>; 2],
    pub bounds: GitFetchBounds,
    pub cancelled: &'a AtomicBool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSubjectUsage {
    pub role: ArtifactId,
    pub resources: GitFetchUsage,
}

/// Acquires both exact base/candidate pairs into independent roots. The first
/// subject is charged before the second receives the aggregate budget that
/// remains.
///
/// # Errors
///
/// Inputs do not reproduce the frozen operator plan, or either exact subject
/// is unavailable, exceeds a subject or aggregate ceiling, or fails commit and
/// tree verification. No partially acquired relation is returned.
pub fn fetch_relation_exact(
    fetch: RelationGitFetch<'_>,
) -> Result<[RelationSubjectUsage; 2], RelationAcquisitionError> {
    let checked = relation_transition(
        fetch.transition.relation.clone(),
        fetch.transition.coordination.clone(),
        fetch.transition.subjects.clone(),
    )?;
    let mut inputs = fetch.subjects;
    inputs.sort_by(|left, right| left.role.cmp(right.role));
    if inputs[0].destination == inputs[1].destination {
        return Err(RelationAcquisitionError::InvalidTransition);
    }

    let plan = checked.relation.plan.as_ref();
    let inputs_valid = inputs
        .iter()
        .zip(&checked.subjects)
        .all(|(input, transition)| {
            plan.subjects
                .iter()
                .find(|subject| subject.role == transition.role)
                .is_some_and(|subject| {
                    let repository = &subject.scope.repository;
                    input.role == &transition.role
                        && input.credential_id == &subject.credential
                        && input.url
                            == format!(
                                "https://{}/{}/{}.git",
                                repository.host(),
                                repository.owner(),
                                repository.name()
                            )
                })
        });
    if !inputs_valid {
        return Err(RelationAcquisitionError::InvalidTransition);
    }

    let mut remaining = GitFetchLimits {
        objects: plan.aggregate_limits.acquisition_objects,
        bytes: plan.aggregate_limits.acquisition_bytes,
    };
    let mut usages = Vec::with_capacity(inputs.len());
    for (input, transition) in inputs.iter().zip(&checked.subjects) {
        let subject = plan
            .subjects
            .iter()
            .find(|subject| subject.role == transition.role)
            .ok_or(RelationAcquisitionError::InvalidTransition)?;
        if subject.object_format != ObjectFormat::Sha1 {
            return Err(RelationAcquisitionError::Unproven);
        }
        let limits = subject_fetch_limits(subject.limits, remaining);
        let resources = fetch_exact(ExactFetch {
            url: input.url,
            wants: &[
                ExactWant {
                    oid: &transition.commits.base,
                    reference: REPOSITORY_TARGET_REF,
                },
                ExactWant {
                    oid: &transition.commits.candidate,
                    reference: REPOSITORY_CANDIDATE_REF,
                },
            ],
            destination: input.destination,
            credential: Some(input.credential),
            bounds: fetch.bounds,
            limits,
            cancelled: fetch.cancelled,
        })
        .map_err(|_defect| RelationAcquisitionError::Unproven)?;
        remaining =
            remaining_after(remaining, resources).ok_or(RelationAcquisitionError::Unproven)?;
        usages.push(RelationSubjectUsage {
            role: transition.role.clone(),
            resources,
        });
    }

    verify_relation_acquired(
        &checked,
        inputs.map(|subject| RelationAcquiredRoot {
            role: subject.role,
            repository: subject.destination,
        }),
    )?;
    usages
        .try_into()
        .map_err(|_defect: Vec<RelationSubjectUsage>| RelationAcquisitionError::InvalidTransition)
}

fn subject_fetch_limits(subject: RelationLimits, remaining: GitFetchLimits) -> GitFetchLimits {
    GitFetchLimits {
        objects: subject.acquisition_objects.min(remaining.objects),
        bytes: subject.acquisition_bytes.min(remaining.bytes),
    }
}

fn remaining_after(remaining: GitFetchLimits, usage: GitFetchUsage) -> Option<GitFetchLimits> {
    Some(GitFetchLimits {
        objects: remaining.objects.checked_sub(usage.objects)?,
        bytes: remaining.bytes.checked_sub(usage.bytes)?,
    })
}
