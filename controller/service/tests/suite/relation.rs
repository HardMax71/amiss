#![expect(
    clippy::unwrap_used,
    reason = "the fixture constructs known-valid relation and provider identities"
)]

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, IntegrationId,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RelationAcquisitionError,
    RelationTransition,
};
use amiss_controller_fixtures::relation::relation_audit;
use amiss_controller_service::{
    CoordinatedRelation, CoordinatedTransition, freeze_relation_transition,
};
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid};

fn delivery(transition: &RelationTransition) -> AuthenticatedDelivery {
    let subject = transition
        .relation
        .plan
        .subjects
        .iter()
        .find(|subject| subject.role == transition.relation.trigger_role)
        .unwrap();
    let frozen = transition
        .subjects
        .iter()
        .find(|frozen| frozen.role == subject.role)
        .unwrap();
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: subject.scope.provider.clone(),
            integration: subject.scope.integration.clone(),
            delivery: DeliveryId::new("delivery/relation".to_owned()).unwrap(),
        },
        change: ChangeLocator {
            provider: subject.scope.provider.clone(),
            repository: subject.scope.repository.clone(),
            change: ChangeId::new("change/relation".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("run/relation".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            subject.object_format,
            frozen.commits.candidate.clone(),
        )
        .unwrap(),
    }
}

#[test]
fn either_authenticated_trigger_freezes_the_same_coordinated_revisions() {
    let source = relation_audit(false).unwrap().transition;
    let source_delivery = delivery(&source);
    assert_eq!(
        freeze_relation_transition(
            CoordinatedRelation {
                delivery: source_delivery.clone(),
                relation: source.relation.clone(),
                coordination: source.coordination.clone(),
            },
            source.subjects.clone(),
        ),
        Ok(CoordinatedTransition {
            delivery: source_delivery,
            transition: source.clone(),
        })
    );

    let mut documentation = source;
    documentation.relation.trigger_role = ArtifactId::new("documentation".to_owned()).unwrap();
    let documentation_delivery = delivery(&documentation);
    assert_eq!(
        freeze_relation_transition(
            CoordinatedRelation {
                delivery: documentation_delivery.clone(),
                relation: documentation.relation.clone(),
                coordination: documentation.coordination.clone(),
            },
            documentation.subjects.clone(),
        ),
        Ok(CoordinatedTransition {
            delivery: documentation_delivery,
            transition: documentation,
        })
    );
}

#[test]
fn an_authenticated_trigger_cannot_freeze_another_candidate() {
    let transition = relation_audit(false).unwrap().transition;
    let delivery = delivery(&transition);
    let mut subjects = transition.subjects.clone();
    subjects
        .iter_mut()
        .find(|subject| subject.role == transition.relation.trigger_role)
        .unwrap()
        .commits
        .candidate = Oid::new(ObjectFormat::Sha1, "0".repeat(40)).unwrap();

    assert_eq!(
        freeze_relation_transition(
            CoordinatedRelation {
                delivery,
                relation: transition.relation,
                coordination: transition.coordination,
            },
            subjects,
        ),
        Err(RelationAcquisitionError::InvalidTransition)
    );
}

#[test]
fn a_direct_stage_value_cannot_bypass_delivery_admission() {
    let transition = relation_audit(false).unwrap().transition;
    let mut delivery = delivery(&transition);
    delivery.identity.integration = IntegrationId::new("installation/other".to_owned()).unwrap();

    assert_eq!(
        freeze_relation_transition(
            CoordinatedRelation {
                delivery,
                relation: transition.relation,
                coordination: transition.coordination,
            },
            transition.subjects,
        ),
        Err(RelationAcquisitionError::InvalidTransition)
    );
}
