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
/// The live registry does not admit the relation and delivery, the revisions do not reproduce that
/// relation, or the trigger candidate differs from the delivery-authenticated candidate.
pub fn freeze_relation_transition(
    registry: &RelationRegistry,
    coordinated: CoordinatedRelation,
    subjects: [RelationSubjectTransition; 2],
) -> Result<CoordinatedTransition, RelationAcquisitionError> {
    let identity = coordinated.relation.plan.identity.clone();
    let CoordinatedRelation {
        delivery,
        relation,
        coordination,
    } = admit_relation_coordination(
        registry,
        coordinated.delivery,
        &identity,
        coordinated.coordination,
    )
    .map_err(|_defect| RelationAcquisitionError::InvalidTransition)?;
    let transition = relation_transition(relation, coordination, subjects)?;
    transition
        .subjects
        .iter()
        .any(|subject| {
            subject.role == transition.relation.trigger_role
                && subject.commits.candidate == delivery.provider_run.candidate_commit
        })
        .then_some(CoordinatedTransition {
            delivery,
            transition,
        })
        .ok_or(RelationAcquisitionError::InvalidTransition)
}
