use amiss_controller::{
    AuthenticatedDelivery, RelationLookupError, RelationRegistry, TriggeredRelation,
    relations_for_delivery,
};
use amiss_wire::model::ArtifactId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinatedRelation {
    pub delivery: AuthenticatedDelivery,
    pub relation: TriggeredRelation,
    pub coordination: ArtifactId,
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
