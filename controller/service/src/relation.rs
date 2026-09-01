use amiss_controller::{
    AuthenticatedDelivery, RelationAcquisitionError, RelationLookupError, RelationRegistry,
    RelationSubjectTransition, RelationTransition, TriggeredRelation, relation_transition,
    relations_for_delivery,
};
use amiss_wire::model::ArtifactId;

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
/// The revisions do not reproduce the operator relation or the trigger candidate differs from the
/// delivery-authenticated candidate.
pub fn freeze_relation_transition(
    coordinated: CoordinatedRelation,
    subjects: [RelationSubjectTransition; 2],
) -> Result<CoordinatedTransition, RelationAcquisitionError> {
    let CoordinatedRelation {
        delivery,
        relation,
        coordination,
    } = coordinated;
    let transition = relation_transition(relation, coordination, subjects)?;
    transition
        .relation
        .plan
        .subjects
        .iter()
        .find(|subject| subject.role == transition.relation.trigger_role)
        .zip(
            transition
                .subjects
                .iter()
                .find(|subject| subject.role == transition.relation.trigger_role),
        )
        .is_some_and(|(subject, frozen)| {
            delivery.identity.provider == delivery.change.provider
                && subject.scope.provider == delivery.identity.provider
                && subject.scope.integration == delivery.identity.integration
                && subject.scope.repository == delivery.change.repository
                && subject.object_format == delivery.provider_run.object_format
                && delivery.provider_run.object_format
                    == delivery.provider_run.candidate_commit.object_format()
                && frozen.commits.candidate == delivery.provider_run.candidate_commit
        })
        .then_some(CoordinatedTransition {
            delivery,
            transition,
        })
        .ok_or(RelationAcquisitionError::InvalidTransition)
}
