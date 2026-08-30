#![cfg(test)]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use amiss_controller::{
    IntegrationId, OidPair, OpaqueId, PlanScope, ProviderIdentity, ProviderInstance,
    ProviderNamespace, RelationAcquisitionError, RelationLimits, RelationPlan,
    RelationStatusDestination, RelationSubject, RelationSubjectTransition, TriggeredRelation,
    relation_transition,
};
use amiss_wire::controls::{ProjectionKind, ProjectionSource, RecordSetSelection};
use amiss_wire::digest::sha256;
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use secrecy::SecretString;

use super::{remaining_after, subject_fetch_limits};
use crate::{
    GitCredential, GitFetchBounds, GitFetchLimits, GitFetchUsage, RelationGitFetch,
    RelationGitSubject, fetch_relation_exact,
};

fn artifact(raw: &str) -> ArtifactId {
    ArtifactId::new(raw.to_owned()).expect("fixed artifact identity")
}

fn opaque(raw: &str) -> OpaqueId {
    OpaqueId::new(raw.to_owned()).expect("fixed opaque identity")
}

fn subject(role: &str, repository: &str) -> RelationSubject {
    RelationSubject {
        role: artifact(role),
        scope: PlanScope {
            provider: ProviderIdentity {
                namespace: ProviderNamespace::new("github".to_owned()).expect("namespace"),
                instance: ProviderInstance::new("github.com".to_owned()).expect("instance"),
            },
            integration: IntegrationId::new(format!("installation/{repository}"))
                .expect("integration"),
            repository: RepositoryIdentity::github("acme".to_owned(), repository.to_owned())
                .expect("repository"),
        },
        target: BranchRef::new("refs/heads/main".to_owned()).expect("branch"),
        object_format: ObjectFormat::Sha1,
        credential: opaque(&format!("credential/{repository}")),
        source: ProjectionSource::RecordSet(RecordSetSelection {
            set: artifact("public/api"),
        }),
        limits: RelationLimits {
            acquisition_objects: 100,
            acquisition_bytes: 1_048_576,
            projection_records: 100,
            projection_bytes: 1_048_576,
        },
    }
}

fn revisions(role: &str, digit: char) -> RelationSubjectTransition {
    RelationSubjectTransition {
        role: artifact(role),
        commits: OidPair {
            base: Oid::new(ObjectFormat::Sha1, digit.to_string().repeat(40)).expect("base commit"),
            candidate: Oid::new(ObjectFormat::Sha1, "c".repeat(40)).expect("candidate commit"),
        },
        trees: OidPair {
            base: Oid::new(ObjectFormat::Sha1, "d".repeat(40)).expect("base tree"),
            candidate: Oid::new(ObjectFormat::Sha1, "e".repeat(40)).expect("candidate tree"),
        },
    }
}

fn transition() -> amiss_controller::RelationTransition {
    let documentation = subject("documentation", "handbook");
    let source = subject("source", "service");
    let plan = Arc::new(RelationPlan {
        identity: artifact("relation/api"),
        context_digest: sha256(b"operator relation context"),
        projection: ProjectionKind::SortedRowsV1,
        subjects: [documentation, source],
        aggregate_limits: RelationLimits {
            acquisition_objects: 150,
            acquisition_bytes: 1_572_864,
            projection_records: 150,
            projection_bytes: 1_572_864,
        },
        status_destinations: vec![RelationStatusDestination {
            subject_role: artifact("documentation"),
            required_status_name: "Amiss cross-repository".to_owned(),
        }],
    });
    relation_transition(
        TriggeredRelation {
            plan,
            trigger_role: artifact("source"),
        },
        artifact("workflow/release-42"),
        [revisions("source", 'a'), revisions("documentation", 'b')],
    )
    .expect("frozen transition")
}

#[test]
fn the_second_subject_receives_only_the_aggregate_budget_left_by_the_first() {
    let subject = RelationLimits {
        acquisition_objects: 100,
        acquisition_bytes: 1_000,
        projection_records: 1,
        projection_bytes: 1,
    };
    let aggregate = GitFetchLimits {
        objects: 150,
        bytes: 1_500,
    };
    assert_eq!(
        subject_fetch_limits(subject, aggregate),
        GitFetchLimits {
            objects: 100,
            bytes: 1_000,
        }
    );
    let remaining = remaining_after(
        aggregate,
        GitFetchUsage {
            objects: 80,
            bytes: 900,
        },
    )
    .expect("usage within the aggregate");
    assert_eq!(
        subject_fetch_limits(subject, remaining),
        GitFetchLimits {
            objects: 70,
            bytes: 600,
        }
    );
    assert!(
        remaining_after(
            remaining,
            GitFetchUsage {
                objects: 71,
                bytes: 1,
            }
        )
        .is_none()
    );
}

#[test]
fn cancellation_makes_the_complete_relation_unproven_without_starting_a_second_fetch() {
    let transition = transition();
    let scratch = tempfile::tempdir().expect("scratch");
    let roots = [
        scratch.path().join("documentation"),
        scratch.path().join("source"),
    ];
    for root in &roots {
        std::fs::create_dir(root).expect("subject root");
    }
    let secrets = [
        SecretString::from("documentation-secret"),
        SecretString::from("source-secret"),
    ];
    let urls = [
        "https://github.com/acme/handbook.git",
        "https://github.com/acme/service.git",
    ];
    let subjects = std::array::from_fn(|index| {
        let subject = &transition.relation.plan.subjects[index];
        RelationGitSubject {
            role: &subject.role,
            credential_id: &subject.credential,
            url: urls[index],
            credential: GitCredential {
                username: "x-access-token",
                password: &secrets[index],
            },
            destination: &roots[index],
        }
    });
    let cancelled = AtomicBool::new(true);

    let result = fetch_relation_exact(RelationGitFetch {
        transition: &transition,
        subjects,
        bounds: GitFetchBounds::default(),
        cancelled: &cancelled,
    });

    assert_eq!(result.err(), Some(RelationAcquisitionError::Unproven));
    for root in roots {
        assert!(root.read_dir().expect("subject entries").next().is_none());
    }
}
