mod acquisition;
mod credential;
mod schedule;
mod status;
mod store;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use amiss_wire::envelope::Envelope;
use amiss_wire::model::{ArtifactId, ObjectFormat};
use amiss_wire::relation::RelationPlan as PlanPayload;

use crate::{
    AuthenticatedDelivery, OpaqueId, PlanScope, ProviderIdentity, RelationAcquisitionError,
    RelationPlan, RelationRegistryError, RelationTransition, TriggeredRelation,
};

use crate::relation_plan::validate_relation;

pub use acquisition::{RelationAcquiredRoot, verify_relation_acquired};
pub use credential::{
    RelationCredentialError, RelationCredentialRoute, RelationCredentialRouter, relation_authority,
    relation_credential_router,
};
pub use schedule::{PendingRelation, RelationAdmission, RelationScheduleError, schedule_relation};
pub use status::{
    RelationStatusError, RelationStatusPublication, RelationStatusRecord, RelationStatusTarget,
    RelationStatusTargets, RelationSubjectHead, complete_relation_status,
    relation_status_publication, relation_status_targets, stage_relation_status,
};
pub use store::{
    FileRelationScheduleStore, RELATION_SCHEDULE_BINDING_LIMIT, RelationScheduleStoreError,
    RelationStatusDeliveryClaim,
};

pub const RELATION_REGISTRY_LIMIT: usize = 1_024;

/// Verifies that one digest-bound audit plan exactly reproduces its frozen
/// operator relation and acquired snapshots.
///
/// # Errors
///
/// The envelope digest or any operator, trigger, subject, or snapshot field
/// differs from the frozen transition.
pub fn verify_relation_plan(
    plan: &Envelope<PlanPayload>,
    transition: &RelationTransition,
) -> Result<(), RelationAcquisitionError> {
    plan.validate()
        .map_err(|_defect| RelationAcquisitionError::InvalidTransition)?;
    let registered = transition.relation.plan.as_ref();
    (plan.payload.relation.identity == registered.identity
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the authenticated relation delivery or declaration is inconsistent")]
pub struct RelationLookupError;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TriggerScope {
    scope: PlanScope,
    object_format: ObjectFormat,
}

pub struct RelationRegistry {
    plans: BTreeMap<ArtifactId, Arc<RelationPlan>>,
    triggers: BTreeMap<TriggerScope, Vec<TriggeredRelation>>,
    credentials: BTreeMap<OpaqueId, (ProviderIdentity, OpaqueId)>,
}

/// Validates and freezes the complete operator-owned registry in one step.
/// Both subjects become trigger owners; no entry can be added or replaced
/// after construction.
///
/// # Errors
///
/// The registry is too large, repeats a relation identity or external status destination, rebinds
/// one credential identity, or contains an invalid subject, selector, budget, or destination.
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
    let mut credentials = BTreeMap::new();
    let mut destinations = BTreeSet::new();
    for plan in &plans {
        for subject in &plan.subjects {
            let scope = (
                subject.scope.provider.clone(),
                subject.scope.integration.clone(),
            );
            credentials
                .get(&subject.credential)
                .is_none_or(|registered| registered == &scope)
                .then_some(())
                .ok_or(RelationRegistryError::ReboundCredential)?;
            credentials.insert(subject.credential.clone(), scope);
        }
        for destination in &plan.status_destinations {
            let subject = plan
                .subjects
                .iter()
                .find(|subject| subject.role == destination.subject_role)
                .ok_or(RelationRegistryError::InvalidDestination)?;
            if !destinations.insert((
                subject.scope.provider.clone(),
                subject.scope.repository.clone(),
                destination.required_status_name.clone(),
            )) {
                return Err(RelationRegistryError::DuplicateDestination);
            }
        }
    }

    let plans = plans
        .into_iter()
        .map(|plan| (plan.identity.clone(), Arc::new(plan)))
        .collect::<BTreeMap<_, _>>();
    let mut triggers: BTreeMap<TriggerScope, Vec<TriggeredRelation>> = BTreeMap::new();
    for plan in plans.values() {
        for subject in &plan.subjects {
            triggers
                .entry(TriggerScope {
                    scope: subject.scope.clone(),
                    object_format: subject.object_format,
                })
                .or_default()
                .push(TriggeredRelation {
                    plan: Arc::clone(plan),
                    trigger_role: subject.role.clone(),
                });
        }
    }
    Ok(RelationRegistry {
        plans,
        triggers,
        credentials,
    })
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
