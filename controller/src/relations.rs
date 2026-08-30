mod acquisition;
mod schedule;
mod store;

use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::controls::{
    ProjectionKind, ProjectionSource, check_projection_source, valid_required_status_name,
};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat};
use amiss_wire::relation::RelationPlanEnvelope;

use crate::{AuthenticatedDelivery, OpaqueId, PlanScope};

pub use acquisition::{
    RelationAcquiredRoot, RelationAcquisitionError, RelationSubjectTransition, RelationTransition,
    relation_transition, verify_relation_acquired,
};
pub use schedule::{PendingRelation, RelationAdmission, RelationScheduleError, schedule_relation};
pub use store::{
    FileRelationScheduleStore, RELATION_SCHEDULE_BINDING_LIMIT, RelationScheduleStoreError,
};

pub const RELATION_REGISTRY_LIMIT: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationLimits {
    pub acquisition_objects: u64,
    pub acquisition_bytes: u64,
    pub projection_records: u64,
    pub projection_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationSubject {
    pub role: ArtifactId,
    pub scope: PlanScope,
    pub target: BranchRef,
    pub object_format: ObjectFormat,
    pub credential: OpaqueId,
    pub source: ProjectionSource,
    pub limits: RelationLimits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationStatusDestination {
    pub subject_role: ArtifactId,
    pub required_status_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationPlan {
    pub identity: ArtifactId,
    pub context_digest: Digest,
    pub projection: ProjectionKind,
    pub subjects: [RelationSubject; 2],
    pub aggregate_limits: RelationLimits,
    pub status_destinations: Vec<RelationStatusDestination>,
}

/// Verifies that one digest-bound audit plan exactly reproduces its frozen
/// operator relation and acquired snapshots.
///
/// # Errors
///
/// The envelope digest or any operator, trigger, subject, or snapshot field
/// differs from the frozen transition.
pub fn verify_relation_plan(
    plan: &RelationPlanEnvelope,
    transition: &RelationTransition,
) -> Result<(), RelationAcquisitionError> {
    let rebuilt = amiss_wire::relation::plan(&plan.payload)
        .map_err(|_defect| RelationAcquisitionError::InvalidTransition)?;
    let registered = transition.relation.plan.as_ref();
    (rebuilt.text("payload_digest") == Some(&plan.payload_digest.to_string())
        && plan.payload.relation.identity == registered.identity
        && plan.payload.relation.context_digest == registered.context_digest
        && plan.payload.coordination == transition.coordination
        && plan.payload.trigger_role == transition.relation.trigger_role
        && plan.payload.projection == registered.projection
        && plan.payload.subjects.iter().all(|planned| {
            registered
                .subjects
                .iter()
                .find(|subject| subject.role == planned.role)
                .zip(
                    transition
                        .subjects
                        .iter()
                        .find(|subject| subject.role == planned.role),
                )
                .is_some_and(|(subject, frozen)| {
                    planned.repository == subject.scope.repository
                        && planned.target == subject.target
                        && planned.source == subject.source
                        && planned.base.commit == frozen.commits.base
                        && planned.base.tree == frozen.trees.base
                        && planned.candidate.commit == frozen.commits.candidate
                        && planned.candidate.tree == frozen.trees.candidate
                })
        }))
    .then_some(())
    .ok_or(RelationAcquisitionError::InvalidTransition)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggeredRelation {
    pub plan: Arc<RelationPlan>,
    pub trigger_role: ArtifactId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the authenticated relation delivery identity is inconsistent")]
pub struct RelationLookupError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationRegistryError {
    #[error("the relation registry exceeds its entry limit")]
    TooManyRelations,
    #[error("a relation identity is registered more than once")]
    DuplicateRelation,
    #[error("a relation does not name two distinct repository subjects and roles")]
    InvalidSubjects,
    #[error("a relation resource budget is invalid")]
    InvalidLimits,
    #[error("a relation projection selector is invalid")]
    InvalidProjection,
    #[error("a relation status destination is invalid")]
    InvalidDestination,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TriggerScope {
    scope: PlanScope,
    object_format: ObjectFormat,
}

pub struct RelationRegistry {
    triggers: BTreeMap<TriggerScope, Vec<TriggeredRelation>>,
}

/// Validates and freezes the complete operator-owned registry in one step.
/// Both subjects become trigger owners; no entry can be added or replaced
/// after construction.
///
/// # Errors
///
/// The registry is too large, repeats an identity, or contains an invalid
/// subject, selector, budget, or status destination.
pub fn relation_registry(
    mut plans: Vec<RelationPlan>,
) -> Result<RelationRegistry, RelationRegistryError> {
    if plans.len() > RELATION_REGISTRY_LIMIT {
        return Err(RelationRegistryError::TooManyRelations);
    }
    for plan in &mut plans {
        plan.subjects
            .sort_by(|left, right| left.role.cmp(&right.role));
        plan.status_destinations
            .sort_by(|left, right| left.subject_role.cmp(&right.subject_role));
        validate_relation(plan)?;
    }
    plans.sort_by(|left, right| left.identity.cmp(&right.identity));
    if plans
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left.identity == right.identity))
    {
        return Err(RelationRegistryError::DuplicateRelation);
    }

    let mut triggers: BTreeMap<TriggerScope, Vec<TriggeredRelation>> = BTreeMap::new();
    for plan in plans {
        let plan = Arc::new(plan);
        for subject in &plan.subjects {
            triggers
                .entry(TriggerScope {
                    scope: subject.scope.clone(),
                    object_format: subject.object_format,
                })
                .or_default()
                .push(TriggeredRelation {
                    plan: Arc::clone(&plan),
                    trigger_role: subject.role.clone(),
                });
        }
    }
    Ok(RelationRegistry { triggers })
}

/// Selects every relation owned by one authenticated provider delivery.
/// A delivery outside the registry is ordinary no-work and returns an empty
/// set. Results are ordered by relation identity.
///
/// # Errors
///
/// The provider or object-format facts disagree inside the authenticated
/// delivery.
pub fn relations_for_delivery(
    registry: &RelationRegistry,
    delivery: &AuthenticatedDelivery,
) -> Result<Vec<TriggeredRelation>, RelationLookupError> {
    if delivery.identity.provider != delivery.change.provider
        || delivery.provider_run.object_format
            != delivery.provider_run.candidate_commit.object_format()
    {
        return Err(RelationLookupError);
    }
    let trigger = TriggerScope {
        scope: PlanScope {
            provider: delivery.identity.provider.clone(),
            integration: delivery.identity.integration.clone(),
            repository: delivery.change.repository.clone(),
        },
        object_format: delivery.provider_run.object_format,
    };
    Ok(registry.triggers.get(&trigger).cloned().unwrap_or_default())
}

fn validate_relation(plan: &RelationPlan) -> Result<(), RelationRegistryError> {
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
            !valid_required_status_name(&destination.required_status_name)
                || !plan
                    .subjects
                    .iter()
                    .any(|subject| subject.role == destination.subject_role)
        })
    {
        return Err(RelationRegistryError::InvalidDestination);
    }
    Ok(())
}

fn relation_limits_valid(plan: &RelationPlan) -> bool {
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
