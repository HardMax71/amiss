#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid relation identities"
)]

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, IntegrationId,
    OpaqueId, PlanScope, ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt,
    ProviderRunId, ProviderRunIdentity, RELATION_REGISTRY_LIMIT, RelationLimits, RelationPlan,
    RelationRegistryError, RelationStatusDestination, RelationSubject, relation_registry,
    relations_for_delivery,
};
use amiss_wire::controls::{
    ProjectionKind, ProjectionSource, RecordSetSelection, RecordValueSelection,
};
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};

fn artifact(raw: &str) -> ArtifactId {
    ArtifactId::new(raw.to_owned()).unwrap()
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.com".to_owned()).unwrap(),
    }
}

fn repository(name: &str) -> RepositoryIdentity {
    RepositoryIdentity::github("acme".to_owned(), name.to_owned()).unwrap()
}

fn scope(name: &str) -> PlanScope {
    PlanScope {
        provider: provider(),
        integration: IntegrationId::new(format!("installation/{name}")).unwrap(),
        repository: repository(name),
    }
}

fn limits(objects: u64, bytes: u64) -> RelationLimits {
    RelationLimits {
        acquisition_objects: objects,
        acquisition_bytes: bytes,
        projection_records: objects,
        projection_bytes: bytes,
    }
}

fn subject(role: &str, repository: &str, set: &str) -> RelationSubject {
    RelationSubject {
        role: artifact(role),
        scope: scope(repository),
        target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        object_format: ObjectFormat::Sha1,
        credential: OpaqueId::new(format!("git/{repository}")).unwrap(),
        source: ProjectionSource::RecordSet(RecordSetSelection { set: artifact(set) }),
        limits: limits(100, 1_048_576),
    }
}

fn plan(identity: &str, source: &str, documentation: &str) -> RelationPlan {
    RelationPlan {
        identity: artifact(identity),
        projection: ProjectionKind::SortedRowsV1,
        subjects: [
            subject("source", source, "rust/public-api"),
            subject("documentation", documentation, "docs/public-api"),
        ],
        aggregate_limits: limits(150, 1_572_864),
        status_destinations: vec![RelationStatusDestination {
            subject_role: artifact("documentation"),
            required_status_name: "Amiss cross-repository".to_owned(),
        }],
    }
}

fn delivery(repository: &str, object_format: ObjectFormat) -> AuthenticatedDelivery {
    let scope = scope(repository);
    let hex = match object_format {
        ObjectFormat::Sha1 => "a".repeat(40),
        ObjectFormat::Sha256 => "a".repeat(64),
    };
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: scope.provider.clone(),
            integration: scope.integration,
            delivery: DeliveryId::new("delivery/1".to_owned()).unwrap(),
        },
        change: ChangeLocator {
            provider: scope.provider,
            repository: scope.repository,
            change: ChangeId::new("pull/7".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("run/9".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            object_format,
            Oid::new(object_format, hex).unwrap(),
        )
        .unwrap(),
    }
}

#[test]
fn either_authenticated_subject_selects_every_owned_relation_in_identity_order() {
    let registry = relation_registry(vec![
        plan("relation/zeta", "sdk", "handbook"),
        plan("relation/alpha", "service", "handbook"),
    ])
    .unwrap();

    let handbook =
        relations_for_delivery(&registry, &delivery("handbook", ObjectFormat::Sha1)).unwrap();
    assert_eq!(
        handbook
            .iter()
            .map(|relation| relation.plan.identity.as_str())
            .collect::<Vec<_>>(),
        ["relation/alpha", "relation/zeta"]
    );
    assert!(
        handbook
            .iter()
            .all(|relation| relation.trigger_role.as_str() == "documentation")
    );

    let source = relations_for_delivery(&registry, &delivery("sdk", ObjectFormat::Sha1)).unwrap();
    let [source] = source.as_slice() else {
        panic!("the source owns exactly one relation");
    };
    assert_eq!(source.plan.identity.as_str(), "relation/zeta");
    assert_eq!(source.trigger_role.as_str(), "source");
}

#[test]
fn unregistered_scope_or_object_format_is_authenticated_no_work() {
    let registry = relation_registry(vec![plan("relation/api", "service", "handbook")]).unwrap();
    assert!(
        relations_for_delivery(&registry, &delivery("other", ObjectFormat::Sha1))
            .unwrap()
            .is_empty()
    );
    assert!(
        relations_for_delivery(&registry, &delivery("service", ObjectFormat::Sha256))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_inconsistent_authenticated_identity_never_selects_a_relation() {
    let registry = relation_registry(vec![plan("relation/api", "service", "handbook")]).unwrap();
    let mut inconsistent = delivery("service", ObjectFormat::Sha1);
    inconsistent.change.provider.instance =
        ProviderInstance::new("elsewhere.test".to_owned()).unwrap();
    assert!(relations_for_delivery(&registry, &inconsistent).is_err());
}

#[test]
fn relation_identities_and_subjects_are_closed_before_indexing() {
    let row = plan("relation/api", "service", "handbook");
    assert_eq!(
        relation_registry(vec![row.clone(), row]).err(),
        Some(RelationRegistryError::DuplicateRelation)
    );

    let mut same_repository = plan("relation/api", "service", "handbook");
    same_repository.subjects[1].scope = same_repository.subjects[0].scope.clone();
    assert_eq!(
        relation_registry(vec![same_repository]).err(),
        Some(RelationRegistryError::InvalidSubjects)
    );

    let mut same_role = plan("relation/api", "service", "handbook");
    same_role.subjects[1].role = same_role.subjects[0].role.clone();
    assert_eq!(
        relation_registry(vec![same_role]).err(),
        Some(RelationRegistryError::InvalidSubjects)
    );

    let too_many = (0..=RELATION_REGISTRY_LIMIT)
        .map(|index| plan(&format!("relation/{index}"), "service", "handbook"))
        .collect();
    assert_eq!(
        relation_registry(too_many).err(),
        Some(RelationRegistryError::TooManyRelations)
    );
}

#[test]
fn selectors_and_budgets_must_be_closed_and_jointly_reachable() {
    let mut incompatible = plan("relation/api", "service", "handbook");
    incompatible.subjects[0].source = ProjectionSource::RecordValue(RecordValueSelection {
        set: artifact("rust/public-api"),
        key: "amiss::check".to_owned(),
    });
    assert_eq!(
        relation_registry(vec![incompatible]).err(),
        Some(RelationRegistryError::InvalidProjection)
    );

    let mut zero = plan("relation/api", "service", "handbook");
    zero.subjects[0].limits.projection_bytes = 0;
    assert_eq!(
        relation_registry(vec![zero]).err(),
        Some(RelationRegistryError::InvalidLimits)
    );

    let mut unreachable = plan("relation/api", "service", "handbook");
    unreachable.aggregate_limits.acquisition_bytes = 1;
    assert_eq!(
        relation_registry(vec![unreachable]).err(),
        Some(RelationRegistryError::InvalidLimits)
    );
}

#[test]
fn status_destinations_are_exact_valid_subjects() {
    let mut unknown = plan("relation/api", "service", "handbook");
    unknown.status_destinations[0].subject_role = artifact("unknown");
    assert_eq!(
        relation_registry(vec![unknown]).err(),
        Some(RelationRegistryError::InvalidDestination)
    );

    let mut repeated = plan("relation/api", "service", "handbook");
    repeated
        .status_destinations
        .push(repeated.status_destinations[0].clone());
    assert_eq!(
        relation_registry(vec![repeated]).err(),
        Some(RelationRegistryError::InvalidDestination)
    );

    let mut malformed = plan("relation/api", "service", "handbook");
    malformed.status_destinations[0].required_status_name = " trailing ".to_owned();
    assert_eq!(
        relation_registry(vec![malformed]).err(),
        Some(RelationRegistryError::InvalidDestination)
    );

    assert!(relation_registry(Vec::new()).is_ok());
}
