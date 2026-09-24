use std::sync::Arc;

use amiss_wire::controls::{ProjectionKind, ProjectionSource, check_projection_source};
use amiss_wire::model::Digest;
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat};

use crate::{OidPair, OpaqueId, PlanScope};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationLimits {
    pub acquisition_objects: u64,
    pub acquisition_bytes: u64,
    pub projection_records: u64,
    pub projection_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredSubject {
    pub role: ArtifactId,
    pub scope: PlanScope,
    pub target: BranchRef,
    pub object_format: ObjectFormat,
    pub credential: OpaqueId,
    pub source: ProjectionSource,
    pub limits: RelationLimits,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationStatusDestination {
    pub subject_role: ArtifactId,
    pub required_status_name: amiss_wire::controls::RequiredStatusName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredRelation {
    pub identity: ArtifactId,
    pub context_digest: Digest,
    pub projection: ProjectionKind,
    pub subjects: [RegisteredSubject; 2],
    pub aggregate_limits: RelationLimits,
    pub status_destinations: Vec<RelationStatusDestination>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggeredRelation {
    pub plan: Arc<RegisteredRelation>,
    pub trigger_role: ArtifactId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationRegistryError {
    #[error("the relation registry exceeds its entry limit")]
    TooManyRelations,
    #[error("a relation identity is registered more than once")]
    DuplicateRelation,
    #[error("a provider repository status destination is owned by more than one relation")]
    DuplicateDestination,
    #[error("a relation credential identity is rebound to another provider authority")]
    ReboundCredential,
    #[error("a relation does not name two distinct repository subjects and roles")]
    InvalidSubjects,
    #[error("a relation resource budget is invalid")]
    InvalidLimits,
    #[error("a relation projection selector is invalid")]
    InvalidProjection,
    #[error("a relation status destination is invalid")]
    InvalidDestination,
}

pub(crate) fn validate_relation(plan: &RegisteredRelation) -> Result<(), RelationRegistryError> {
    let [left, right] = &plan.subjects;
    if left.role == right.role || left.scope.repository == right.scope.repository {
        return Err(RelationRegistryError::InvalidSubjects);
    }
    if !relation_limits_valid(plan) {
        return Err(RelationRegistryError::InvalidLimits);
    }
    if plan
        .subjects
        .iter()
        .any(|subject| check_projection_source(plan.projection, &subject.source).is_err())
    {
        return Err(RelationRegistryError::InvalidProjection);
    }
    if !(1..=plan.subjects.len()).contains(&plan.status_destinations.len())
        || plan
            .status_destinations
            .windows(2)
            .any(|pair| matches!(pair, [left, right] if left.subject_role == right.subject_role))
        || plan.status_destinations.iter().any(|destination| {
            !plan
                .subjects
                .iter()
                .any(|subject| subject.role == destination.subject_role)
        })
    {
        return Err(RelationRegistryError::InvalidDestination);
    }
    Ok(())
}

fn relation_limits_valid(plan: &RegisteredRelation) -> bool {
    let [left, right] = &plan.subjects;
    [
        (
            plan.aggregate_limits.acquisition_objects,
            left.limits.acquisition_objects,
            right.limits.acquisition_objects,
        ),
        (
            plan.aggregate_limits.acquisition_bytes,
            left.limits.acquisition_bytes,
            right.limits.acquisition_bytes,
        ),
        (
            plan.aggregate_limits.projection_records,
            left.limits.projection_records,
            right.limits.projection_records,
        ),
        (
            plan.aggregate_limits.projection_bytes,
            left.limits.projection_bytes,
            right.limits.projection_bytes,
        ),
    ]
    .into_iter()
    .all(|(aggregate, left, right)| {
        left != 0
            && right != 0
            && left
                .checked_add(right)
                .is_some_and(|total| (left.max(right)..=total).contains(&aggregate))
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSubjectTransition {
    pub role: ArtifactId,
    pub commits: OidPair,
    pub trees: OidPair,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationTransition {
    pub relation: TriggeredRelation,
    pub coordination: ArtifactId,
    pub subjects: [RelationSubjectTransition; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationAcquisitionError {
    #[error("the frozen relation transition is inconsistent with its operator plan")]
    InvalidTransition,
    #[error("one or more exact relation subjects cannot be proven")]
    Unproven,
}

/// Freezes two independently resolved base/candidate pairs against one
/// operator-owned relation selected by an authenticated trigger. `coordination`
/// is the trusted operator's opaque identity for the exact pair, release, or
/// workflow occurrence; this function never derives one from revisions or time.
///
/// # Errors
///
/// The relation, trigger role, subject roles, or object formats do not exactly
/// reproduce the registered plan.
pub fn relation_transition(
    relation: TriggeredRelation,
    coordination: ArtifactId,
    mut subjects: [RelationSubjectTransition; 2],
) -> Result<RelationTransition, RelationAcquisitionError> {
    subjects.sort_by(|left, right| left.role.cmp(&right.role));
    let plan = relation.plan.as_ref();
    let relation_valid = validate_relation(plan).is_ok()
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
    Ok(RelationTransition {
        relation,
        coordination,
        subjects,
    })
}
