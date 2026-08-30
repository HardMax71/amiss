#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid relation identities"
)]

use std::sync::Arc;

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, IntegrationId,
    LeaseFence, OidPair, OpaqueId, PendingRelation, PlanScope, ProviderIdentity, ProviderInstance,
    ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity,
    RELATION_REGISTRY_LIMIT, RelationAcquiredRoot, RelationAcquisitionError, RelationAdmission,
    RelationLimits, RelationPlan, RelationRegistryError, RelationScheduleError,
    RelationStatusDestination, RelationStatusError, RelationSubject, RelationSubjectHead,
    RelationSubjectTransition, RelationTransition, relation_registry, relation_status_targets,
    relation_transition, relations_for_delivery, schedule_relation, verify_relation_acquired,
};
use amiss_fixtures::{CommitPair, commit_pair, git};
use amiss_wire::controls::{
    ProjectionKind, ProjectionSource, RecordSetSelection, RecordValueSelection,
};
use amiss_wire::digest::sha256;
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
        context_digest: sha256(identity.as_bytes()),
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

fn tree(pair: &CommitPair, commit: &str) -> Oid {
    let revision = format!("{commit}^{{tree}}");
    Oid::new(
        ObjectFormat::Sha1,
        git(pair.root(), &["rev-parse", &revision])
            .unwrap()
            .trim()
            .to_owned(),
    )
    .unwrap()
}

fn transition_subject(role: &str, pair: &CommitPair) -> RelationSubjectTransition {
    RelationSubjectTransition {
        role: artifact(role),
        commits: OidPair {
            base: Oid::new(ObjectFormat::Sha1, pair.base.clone()).unwrap(),
            candidate: Oid::new(ObjectFormat::Sha1, pair.candidate.clone()).unwrap(),
        },
        trees: OidPair {
            base: tree(pair, &pair.base),
            candidate: tree(pair, &pair.candidate),
        },
    }
}

fn frozen_transition(source: &CommitPair, documentation: &CommitPair) -> RelationTransition {
    let registry = relation_registry(vec![plan("relation/api", "service", "handbook")]).unwrap();
    let relation = relations_for_delivery(&registry, &delivery("service", ObjectFormat::Sha1))
        .unwrap()
        .pop()
        .unwrap();
    relation_transition(
        relation,
        artifact("workflow/release-42"),
        [
            transition_subject("source", source),
            transition_subject("documentation", documentation),
        ],
    )
    .unwrap()
}

fn current_heads(transition: &RelationTransition) -> [RelationSubjectHead; 2] {
    let plan = transition.relation.plan.as_ref();
    transition.subjects.each_ref().map(|frozen| {
        let subject = plan
            .subjects
            .iter()
            .find(|subject| subject.role == frozen.role)
            .unwrap();
        RelationSubjectHead {
            subject: subject.clone(),
            candidate_commit: frozen.commits.candidate.clone(),
        }
    })
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

#[test]
fn freezes_all_four_exact_revisions_in_stable_role_order() {
    let source = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let documentation = commit_pair(&[("api", "v1")], &[("api", "v1")]).unwrap();
    let transition = frozen_transition(&source, &documentation);

    assert_eq!(transition.coordination.as_str(), "workflow/release-42");
    assert_eq!(transition.subjects[0].role.as_str(), "documentation");
    assert_eq!(transition.subjects[1].role.as_str(), "source");
    assert_eq!(
        transition.subjects[1].commits.candidate.as_str(),
        source.candidate
    );
}

#[test]
fn acquired_relation_roots_prove_each_subjects_commits_and_trees() {
    let source = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let documentation = commit_pair(&[("api", "v1")], &[("api", "v1")]).unwrap();
    let transition = frozen_transition(&source, &documentation);
    let source_role = artifact("source");
    let documentation_role = artifact("documentation");

    assert!(
        verify_relation_acquired(
            &transition,
            [
                RelationAcquiredRoot {
                    role: &source_role,
                    repository: source.root(),
                },
                RelationAcquiredRoot {
                    role: &documentation_role,
                    repository: documentation.root(),
                },
            ],
        )
        .is_ok()
    );
    assert_eq!(
        verify_relation_acquired(
            &transition,
            [
                RelationAcquiredRoot {
                    role: &source_role,
                    repository: documentation.root(),
                },
                RelationAcquiredRoot {
                    role: &documentation_role,
                    repository: source.root(),
                },
            ],
        )
        .err(),
        Some(RelationAcquisitionError::Unproven)
    );
}

#[test]
fn identical_object_ids_still_require_independent_subject_roots() {
    let shared = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let transition = frozen_transition(&shared, &shared);
    let source_role = artifact("source");
    let documentation_role = artifact("documentation");

    assert_eq!(
        verify_relation_acquired(
            &transition,
            [
                RelationAcquiredRoot {
                    role: &source_role,
                    repository: shared.root(),
                },
                RelationAcquiredRoot {
                    role: &documentation_role,
                    repository: shared.root(),
                },
            ],
        )
        .err(),
        Some(RelationAcquisitionError::Unproven)
    );
}

#[test]
fn exact_relation_work_is_pending_once_and_duplicate_from_either_trigger() {
    let source = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let documentation = commit_pair(&[("api", "v1")], &[("api", "v1")]).unwrap();
    let transition = frozen_transition(&source, &documentation);
    let RelationAdmission::Scheduled(first) = schedule_relation(None, transition.clone()).unwrap()
    else {
        panic!("first exact work schedules");
    };
    assert_eq!(first.fence.get(), 1);

    let mut opposite_trigger = transition;
    opposite_trigger.relation.trigger_role = artifact("documentation");
    let RelationAdmission::Duplicate(repeated) =
        schedule_relation(Some(first.clone()), opposite_trigger).unwrap()
    else {
        panic!("the other authenticated trigger deduplicates exact work");
    };
    assert_eq!(repeated, first);
}

#[test]
fn current_subject_heads_freeze_only_operator_status_destinations() {
    let source = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let documentation = commit_pair(&[("api", "v1")], &[("api", "v1")]).unwrap();
    let mut transition = frozen_transition(&source, &documentation);
    transition.relation.trigger_role = artifact("documentation");
    let RelationAdmission::Scheduled(pending) = schedule_relation(None, transition).unwrap() else {
        panic!("exact work schedules");
    };
    let mut heads = current_heads(&pending.transition);
    heads.swap(0, 1);

    let targets = relation_status_targets(&pending, heads).unwrap();
    assert_eq!(targets.relation.as_str(), "relation/api");
    assert_eq!(targets.coordination.as_str(), "workflow/release-42");
    assert_eq!(targets.trigger_role.as_str(), "documentation");
    assert_eq!(targets.fence, pending.fence);
    let [destination] = targets.destinations.as_slice() else {
        panic!("only the configured documentation role receives a status");
    };
    assert_eq!(destination.subject.role.as_str(), "documentation");
    assert_eq!(destination.subject.scope, scope("handbook"));
    assert_eq!(destination.subject.target.as_str(), "refs/heads/main");
    assert_eq!(destination.subject.object_format, ObjectFormat::Sha1);
    assert_eq!(destination.subject.credential.as_str(), "git/handbook");
    assert_eq!(
        destination.candidate_commit.as_str(),
        documentation.candidate
    );
    assert_eq!(destination.required_status_name, "Amiss cross-repository");
}

#[test]
fn status_targets_fail_closed_on_moved_or_malformed_finality() {
    let source = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let documentation = commit_pair(&[("api", "v1")], &[("api", "v1")]).unwrap();
    let transition = frozen_transition(&source, &documentation);
    let RelationAdmission::Scheduled(pending) = schedule_relation(None, transition).unwrap() else {
        panic!("exact work schedules");
    };
    let heads = current_heads(&pending.transition);

    let mut moved = heads.clone();
    let source = moved
        .iter_mut()
        .find(|head| head.subject.role.as_str() == "source")
        .unwrap();
    source.candidate_commit = Oid::new(ObjectFormat::Sha1, "9".repeat(40)).unwrap();
    assert_eq!(
        relation_status_targets(&pending, moved).unwrap_err(),
        RelationStatusError::Superseded
    );

    let mut repeated = heads.clone();
    repeated[1].subject = repeated[0].subject.clone();
    assert_eq!(
        relation_status_targets(&pending, repeated).unwrap_err(),
        RelationStatusError::InvalidHeads
    );

    let mut rebound = heads.clone();
    rebound[0].subject.credential = OpaqueId::new("git/other".to_owned()).unwrap();
    assert_eq!(
        relation_status_targets(&pending, rebound).unwrap_err(),
        RelationStatusError::InvalidHeads
    );

    let mut invalid = pending;
    invalid.transition.relation.trigger_role = artifact("unknown");
    assert_eq!(
        relation_status_targets(&invalid, heads).unwrap_err(),
        RelationStatusError::InvalidTransition
    );
}

#[test]
fn a_coordination_identity_cannot_be_rebound_but_a_new_one_supersedes() {
    let source = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let documentation = commit_pair(&[("api", "v1")], &[("api", "v1")]).unwrap();
    let transition = frozen_transition(&source, &documentation);
    let RelationAdmission::Scheduled(first) = schedule_relation(None, transition.clone()).unwrap()
    else {
        panic!("first exact work schedules");
    };

    let mut rebound = transition.clone();
    rebound.subjects[1].trees.candidate = rebound.subjects[1].trees.base.clone();
    assert_eq!(
        schedule_relation(Some(first.clone()), rebound).unwrap_err(),
        RelationScheduleError::CoordinationConflict
    );

    let mut next = transition;
    next.coordination = artifact("workflow/release-43");
    let RelationAdmission::Scheduled(next) = schedule_relation(Some(first), next).unwrap() else {
        panic!("a different declared coordination schedules new work");
    };
    assert_eq!(next.fence.get(), 2);
}

#[test]
fn scheduling_refuses_configuration_rebinding_and_fence_overflow() {
    let source = commit_pair(&[("api", "v1")], &[("api", "v2")]).unwrap();
    let documentation = commit_pair(&[("api", "v1")], &[("api", "v1")]).unwrap();
    let transition = frozen_transition(&source, &documentation);
    let previous = PendingRelation {
        transition: transition.clone(),
        fence: LeaseFence::new(1).unwrap(),
    };
    let mut rebound = transition.clone();
    let mut plan = rebound.relation.plan.as_ref().clone();
    plan.context_digest = sha256(b"another operator relation context");
    rebound.relation.plan = Arc::new(plan);
    assert_eq!(
        schedule_relation(Some(previous), rebound).unwrap_err(),
        RelationScheduleError::BindingConflict
    );

    let exhausted = PendingRelation {
        transition: transition.clone(),
        fence: LeaseFence::new(u64::MAX).unwrap(),
    };
    let mut next = transition;
    next.coordination = artifact("workflow/release-43");
    assert_eq!(
        schedule_relation(Some(exhausted), next).unwrap_err(),
        RelationScheduleError::GenerationExhausted
    );
}
