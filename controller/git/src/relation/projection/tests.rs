#![cfg(test)]

use std::sync::Arc;

use amiss_controller::{
    IntegrationId, OidPair, OpaqueId, PlanScope, ProviderIdentity, ProviderInstance,
    ProviderNamespace, RelationAcquiredRoot, RelationLimits, RelationPlan,
    RelationStatusDestination, RelationSubject, RelationSubjectTransition, RelationTransition,
    TriggeredRelation, relation_transition,
};
use amiss_wire::controls::{BlobLineSelection, ProjectionKind, ProjectionSource};
use amiss_wire::digest::sha256;
use amiss_wire::json;
use amiss_wire::model::{
    ArtifactId, BranchRef, ObjectFormat, Oid, RepoPathText, RepositoryIdentity,
};
use amiss_wire::relation::{
    RelationIdentity, RelationPlanEnvelope, RelationSnapshot, RelationSubject as PlannedSubject,
    RelationVerdict, assess, parse_evidence, parse_plan, plan,
};

use super::{RelationProjectionError, RelationProjectionRequest, project_relation_evidence};

struct Fixture {
    source: amiss_fixtures::CommitPair,
    documentation: amiss_fixtures::CommitPair,
    transition: RelationTransition,
    plan: RelationPlanEnvelope,
}

fn artifact(raw: &str) -> ArtifactId {
    ArtifactId::new(raw.to_owned()).expect("fixed artifact identity")
}

fn path(raw: &str) -> RepoPathText {
    RepoPathText::new(raw.to_owned()).expect("fixed repository path")
}

fn repository(name: &str) -> RepositoryIdentity {
    RepositoryIdentity::github("acme".to_owned(), name.to_owned()).expect("fixed repository")
}

fn subject(role: &str, repository_name: &str, source_path: &str) -> RelationSubject {
    RelationSubject {
        role: artifact(role),
        scope: PlanScope {
            provider: ProviderIdentity {
                namespace: ProviderNamespace::new("github".to_owned()).expect("namespace"),
                instance: ProviderInstance::new("github.com".to_owned()).expect("instance"),
            },
            integration: IntegrationId::new(format!("installation/{repository_name}"))
                .expect("integration"),
            repository: repository(repository_name),
        },
        target: BranchRef::new("refs/heads/main".to_owned()).expect("branch"),
        object_format: ObjectFormat::Sha1,
        credential: OpaqueId::new(format!("credential/{repository_name}")).expect("credential"),
        source: ProjectionSource::BlobLines(BlobLineSelection {
            path: path(source_path),
            first_line: 1,
            last_line: 1,
        }),
        limits: RelationLimits {
            acquisition_objects: 100,
            acquisition_bytes: 1_048_576,
            projection_records: 2,
            projection_bytes: 1_024,
        },
    }
}

fn frozen(role: &str, pair: &amiss_fixtures::CommitPair) -> RelationSubjectTransition {
    RelationSubjectTransition {
        role: artifact(role),
        commits: OidPair {
            base: Oid::new(ObjectFormat::Sha1, pair.base.clone()).expect("base commit"),
            candidate: Oid::new(ObjectFormat::Sha1, pair.candidate.clone())
                .expect("candidate commit"),
        },
        trees: OidPair {
            base: Oid::new(ObjectFormat::Sha1, pair.base_tree.clone()).expect("base tree"),
            candidate: Oid::new(ObjectFormat::Sha1, pair.candidate_tree.clone())
                .expect("candidate tree"),
        },
    }
}

fn fixture(aggregate_records: u64) -> Fixture {
    let source = amiss_fixtures::commit_pair(
        &[("api.txt", "timeout: u64\n")],
        &[("api.txt", "timeout: u128\n")],
    )
    .expect("source repository");
    let documentation = amiss_fixtures::commit_pair(&[("mirror.txt", "timeout: u64\n")], &[])
        .expect("documentation repository");
    let registered = Arc::new(RelationPlan {
        identity: artifact("relation/api"),
        projection: ProjectionKind::CodeTextV1,
        subjects: [
            subject("documentation", "handbook", "mirror.txt"),
            subject("source", "service", "api.txt"),
        ],
        aggregate_limits: RelationLimits {
            acquisition_objects: 150,
            acquisition_bytes: 1_572_864,
            projection_records: aggregate_records,
            projection_bytes: 2_048,
        },
        status_destinations: vec![RelationStatusDestination {
            subject_role: artifact("documentation"),
            required_status_name: "Amiss cross-repository".to_owned(),
        }],
    });
    let transition = relation_transition(
        TriggeredRelation {
            plan: Arc::clone(&registered),
            trigger_role: artifact("source"),
        },
        [
            frozen("source", &source),
            frozen("documentation", &documentation),
        ],
    )
    .expect("frozen relation transition");
    let subjects = transition.subjects.clone().map(|frozen| {
        let configured = registered
            .subjects
            .iter()
            .find(|subject| subject.role == frozen.role)
            .expect("registered role");
        PlannedSubject {
            role: frozen.role,
            repository: configured.scope.repository.clone(),
            target: configured.target.clone(),
            source: configured.source.clone(),
            base: RelationSnapshot {
                commit: frozen.commits.base,
                tree: frozen.trees.base,
            },
            candidate: RelationSnapshot {
                commit: frozen.commits.candidate,
                tree: frozen.trees.candidate,
            },
        }
    });
    let value = plan(&amiss_wire::relation::RelationPlan {
        report_payload_digest: sha256(b"accepted report payload"),
        relation: RelationIdentity {
            identity: registered.identity.clone(),
            context_digest: sha256(b"operator relation context"),
        },
        trigger_role: transition.relation.trigger_role.clone(),
        projection: registered.projection,
        subjects,
    })
    .expect("relation plan");
    let plan = parse_plan(&json::canonical(&value)).expect("parsed relation plan");
    Fixture {
        source,
        documentation,
        transition,
        plan,
    }
}

fn roots(fixture: &Fixture) -> [RelationAcquiredRoot<'_>; 2] {
    let [documentation, source] = &fixture.transition.subjects;
    [
        RelationAcquiredRoot {
            role: &documentation.role,
            repository: fixture.documentation.root(),
        },
        RelationAcquiredRoot {
            role: &source.role,
            repository: fixture.source.root(),
        },
    ]
}

#[test]
fn four_exact_repository_projections_produce_the_plan_bound_transition() {
    let fixture = fixture(4);
    let value = project_relation_evidence(RelationProjectionRequest {
        transition: &fixture.transition,
        plan: &fixture.plan,
        roots: roots(&fixture),
    })
    .expect("complete projection evidence");
    let evidence = parse_evidence(&json::canonical(&value)).expect("parsed evidence");
    let [documentation, source] = &evidence.payload.subjects;

    assert_eq!(documentation.base, documentation.candidate);
    assert_eq!(documentation.base, source.base);
    assert_ne!(source.base, source.candidate);
    assert_eq!(documentation.role.as_str(), "documentation");
    assert_eq!(source.role.as_str(), "source");
    assert_eq!(
        evidence.payload.plan_payload_digest,
        fixture.plan.payload_digest
    );
    let assessment = assess(
        &fixture.plan,
        Some(&evidence),
        "0.26.0-test",
        sha256(b"relation evaluator"),
    )
    .expect("transition assessment");
    assert_eq!(
        assessment
            .member("payload")
            .and_then(|payload| payload.text("verdict")),
        Some(RelationVerdict::IntroducedDrift.as_ref())
    );
}

#[test]
fn changed_plan_fields_and_aliased_roots_are_refused_before_projection() {
    let fixture = fixture(4);
    let mut changed = fixture.plan.payload.clone();
    changed.subjects[0].source = ProjectionSource::BlobLines(BlobLineSelection {
        path: path("other.txt"),
        first_line: 1,
        last_line: 1,
    });
    let changed = plan(&changed).expect("rewritten plan");
    let changed = parse_plan(&json::canonical(&changed)).expect("parsed rewritten plan");
    assert_eq!(
        project_relation_evidence(RelationProjectionRequest {
            transition: &fixture.transition,
            plan: &changed,
            roots: roots(&fixture),
        })
        .unwrap_err(),
        RelationProjectionError::InvalidPlan
    );

    let [documentation, source] = &fixture.transition.subjects;
    assert_eq!(
        project_relation_evidence(RelationProjectionRequest {
            transition: &fixture.transition,
            plan: &fixture.plan,
            roots: [
                RelationAcquiredRoot {
                    role: &documentation.role,
                    repository: fixture.documentation.root(),
                },
                RelationAcquiredRoot {
                    role: &source.role,
                    repository: fixture.documentation.root(),
                },
            ],
        })
        .unwrap_err(),
        RelationProjectionError::Unproven
    );
}

#[test]
fn subject_and_aggregate_budgets_cover_both_snapshots_before_the_next_role() {
    let fixture = fixture(3);
    assert_eq!(
        project_relation_evidence(RelationProjectionRequest {
            transition: &fixture.transition,
            plan: &fixture.plan,
            roots: roots(&fixture),
        })
        .unwrap_err(),
        RelationProjectionError::Unproven
    );
}
