#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed provider fixtures must fail loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};

use amiss_controller::{
    ArtifactAuditDigests, ArtifactAuditReference, ArtifactReference, IntegrationId, LeaseFence,
    ProviderError, ProviderInstance, RelationAuditBundle, RelationStatusRecord,
    RelationStatusTarget, RelationStatusTargets, RelationSubject, validate_relation_audit,
};
use amiss_controller_fixtures::relation::{RelationAuditFixture, relation_audit};
use amiss_wire::digest::{Digest, sha256};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::relation::{RelationSnapshot, RelationVerdict};

use super::{GitHubRelationRest, RelationSubjectHead, relation_check_run};
use crate::live::model::{
    CheckRunApp, CheckRunOutputRecord, CheckRunRecord, CommitRecord, CreateCheckRun,
};
use crate::live::publication::{CheckRunDecision, check_run_decision};
use crate::live::{Client, Config};

const APP_ID: u64 = 99;
const INSTALLATION_ID: u64 = 7;

#[test]
fn exact_subject_resolves_one_current_head() {
    let (config, subject) = fixture();
    let client = Client {
        config,
        rest: FakeRelationRest::new(Ok(CommitRecord {
            sha: "3".repeat(40),
            tree: "4".repeat(40),
        })),
    };

    assert_eq!(
        client.resolve_relation_head(&subject),
        Ok(RelationSubjectHead {
            subject,
            candidate: RelationSnapshot {
                commit: oid(ObjectFormat::Sha1, '3'),
                tree: oid(ObjectFormat::Sha1, '4'),
            },
        })
    );
    assert_eq!(client.rest.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn request_scope_is_checked_before_provider_io() {
    let (config, subject) = fixture();
    let defects: [fn(&mut RelationSubject); 6] = [
        |subject| {
            subject.scope.provider.instance =
                ProviderInstance::new("github.example".to_owned()).unwrap();
        },
        |subject| {
            subject.scope.integration = IntegrationId::new("8".to_owned()).unwrap();
        },
        |subject| {
            subject.scope.repository = RepositoryIdentity::new(
                "github.example".to_owned(),
                "acme".to_owned(),
                "handbook".to_owned(),
            )
            .unwrap();
        },
        |subject| {
            subject.scope.repository = RepositoryIdentity::new(
                "github.com".to_owned(),
                "group/acme".to_owned(),
                "handbook".to_owned(),
            )
            .unwrap();
        },
        |subject| subject.object_format = ObjectFormat::Sha256,
        |subject| {
            subject.scope.provider.namespace =
                amiss_controller::ProviderNamespace::new("gitlab".to_owned()).unwrap();
        },
    ];

    for defect in defects {
        let mut changed = subject.clone();
        defect(&mut changed);
        let client = Client {
            config: config.clone(),
            rest: FakeRelationRest::new(Ok(CommitRecord {
                sha: "3".repeat(40),
                tree: "4".repeat(40),
            })),
        };
        assert_eq!(
            client.resolve_relation_head(&changed),
            Err(ProviderError::InvalidResponse)
        );
        assert_eq!(client.rest.calls.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn malformed_head_or_provider_failure_is_not_a_finality_fact() {
    let (config, subject) = fixture();
    for head in [
        Ok(CommitRecord {
            sha: "not-an-oid".to_owned(),
            tree: "4".repeat(40),
        }),
        Ok(CommitRecord {
            sha: "3".repeat(40),
            tree: "not-an-oid".to_owned(),
        }),
        Err(ProviderError::Unavailable),
    ] {
        let expected = head
            .as_ref()
            .err()
            .copied()
            .unwrap_or(ProviderError::InvalidResponse);
        let client = Client {
            config: config.clone(),
            rest: FakeRelationRest::new(head),
        };
        assert_eq!(client.resolve_relation_head(&subject), Err(expected));
    }
}

#[test]
fn relation_check_run_binds_the_exact_audit_without_exposing_the_credential() {
    let (config, mut status, target) = status_fixture();
    let cases = [
        (RelationVerdict::Aligned, "success"),
        (RelationVerdict::IntroducedDrift, "failure"),
        (RelationVerdict::PreExistingDrift, "failure"),
        (RelationVerdict::ResolvedDrift, "success"),
        (RelationVerdict::Unproven, "failure"),
    ];
    let mut identities = std::collections::BTreeSet::new();
    for (verdict, conclusion) in cases {
        let ArtifactAuditDigests::Relation(mut audit) = status.audit.audit else {
            panic!("the fixture carries a relation audit");
        };
        audit.verdict = verdict;
        status.audit.audit = ArtifactAuditDigests::Relation(audit);
        let expected = relation_check_run(&config, &status, &target).unwrap();
        assert_eq!(expected.name, target.required_status_name);
        assert_eq!(expected.head_sha, target.candidate_commit.as_str());
        assert_eq!(expected.conclusion, conclusion);
        assert_eq!(expected.status, "completed");
        assert!(Digest::from_wire(&expected.external_id).is_some());
        assert!(identities.insert(expected.external_id));
        for binding in [
            format!("relation: {}", status.targets.relation.as_str()),
            format!("coordination: {}", status.targets.coordination.as_str()),
            format!("fence: {}", status.targets.fence.get()),
            format!("verdict: {}", verdict.as_ref()),
            format!("destination-role: {}", target.role.as_str()),
            format!("candidate-commit: {}", target.candidate_commit.as_str()),
            format!("report: {}", audit.report_digest),
            format!("plan: {}", audit.plan_digest),
            format!("assessment: {}", audit.assessment_digest),
        ] {
            assert!(
                expected.output.summary.lines().any(|line| line == binding),
                "missing relation binding: {binding}"
            );
        }
        assert!(!expected.output.summary.contains(target.credential.as_str()));
    }
}

#[test]
fn relation_status_mutations_are_rejected_before_reconciliation() {
    let (config, status, target) = status_fixture();

    let mut completed = status.clone();
    completed.completed = true;
    assert!(matches!(
        relation_check_run(&config, &completed, &target),
        Err(ProviderError::InvalidResponse)
    ));

    let mut duplicated = status.clone();
    duplicated.targets.destinations.push(target.clone());
    assert!(matches!(
        relation_check_run(&config, &duplicated, &target),
        Err(ProviderError::InvalidResponse)
    ));

    let mut foreign = target.clone();
    foreign.scope.integration = IntegrationId::new("8".to_owned()).unwrap();
    let mut foreign_status = status.clone();
    foreign_status.targets.destinations[0] = foreign.clone();
    assert!(matches!(
        relation_check_run(&config, &foreign_status, &foreign),
        Err(ProviderError::InvalidResponse)
    ));

    let ArtifactAuditDigests::Relation(mut incomplete_audit) = status.audit.audit else {
        panic!("the fixture carries a relation audit");
    };
    incomplete_audit.evidence_digest = None;
    let mut incomplete = status.clone();
    incomplete.audit.audit = ArtifactAuditDigests::Relation(incomplete_audit);
    assert!(matches!(
        relation_check_run(&config, &incomplete, &target),
        Err(ProviderError::InvalidResponse)
    ));

    let mut substituted = status;
    substituted.audit.artifact.report_digest = sha256(b"other report");
    assert!(matches!(
        relation_check_run(&config, &substituted, &target),
        Err(ProviderError::InvalidResponse)
    ));
}

#[test]
fn relation_check_run_reconciliation_reuses_only_one_exact_result() {
    let (config, status, target) = status_fixture();
    let expected = relation_check_run(&config, &status, &target).unwrap();
    let run = check_run(APP_ID, &expected);
    assert!(matches!(
        check_run_decision(&config, expected.clone(), std::slice::from_ref(&run)),
        Ok(CheckRunDecision::Reuse)
    ));
    assert!(matches!(
        check_run_decision(&config, expected.clone(), &[]),
        Ok(CheckRunDecision::Create(created)) if created.external_id == expected.external_id
    ));
    assert!(matches!(
        check_run_decision(&config, expected.clone(), &[run.clone(), run]),
        Err(ProviderError::InvalidResponse)
    ));

    let mut conflicting = check_run(APP_ID, &expected);
    conflicting.output.summary = Some("different audit".to_owned());
    assert!(matches!(
        check_run_decision(&config, expected, &[conflicting]),
        Err(ProviderError::InvalidResponse)
    ));
}

fn fixture() -> (Config, RelationSubject) {
    let relation = relation_audit(true).unwrap();
    let mut subject = relation
        .transition
        .relation
        .plan
        .subjects
        .iter()
        .find(|subject| subject.scope.provider.namespace.as_str() == "github")
        .unwrap()
        .clone();
    subject.scope.integration = IntegrationId::new(INSTALLATION_ID.to_string()).unwrap();
    (
        Config {
            provider: subject.scope.provider.clone(),
            app_id: APP_ID,
            installation_id: INSTALLATION_ID,
            required_status_name: "amiss/provider".to_owned(),
        },
        subject,
    )
}

fn status_fixture() -> (Config, RelationStatusRecord, RelationStatusTarget) {
    let fixture = relation_audit(true).unwrap();
    let mut subject = fixture
        .transition
        .relation
        .plan
        .subjects
        .iter()
        .find(|subject| subject.scope.provider.namespace.as_str() == "github")
        .unwrap()
        .clone();
    subject.scope.integration = IntegrationId::new(INSTALLATION_ID.to_string()).unwrap();
    let frozen = fixture
        .transition
        .subjects
        .iter()
        .find(|frozen| frozen.role == subject.role)
        .unwrap();
    let target = RelationStatusTarget {
        role: subject.role,
        scope: subject.scope,
        credential: subject.credential,
        candidate_commit: frozen.commits.candidate.clone(),
        required_status_name: "Amiss cross-repository".to_owned(),
    };
    let audit = validate_relation_audit(audit_bundle(&fixture)).unwrap();
    let status = RelationStatusRecord {
        targets: RelationStatusTargets {
            relation: fixture.transition.relation.plan.identity.clone(),
            coordination: fixture.transition.coordination.clone(),
            trigger_role: fixture.transition.relation.trigger_role.clone(),
            fence: LeaseFence::new(7).unwrap(),
            destinations: vec![target.clone()],
        },
        audit: ArtifactAuditReference {
            artifact: ArtifactReference {
                id: "a".repeat(64),
                locator: format!("https://amiss.example/artifacts/{}/report", "a".repeat(64)),
                expires_at_unix_millis: 1_800_000_000_000,
                report_digest: audit.report_digest,
                semantic_digest: None,
                assessment_digest: None,
                external_tally: None,
                external_incomplete: false,
            },
            audit: ArtifactAuditDigests::Relation(audit),
        },
        completed: false,
    };
    (
        Config {
            provider: target.scope.provider.clone(),
            app_id: APP_ID,
            installation_id: INSTALLATION_ID,
            required_status_name: "amiss/provider".to_owned(),
        },
        status,
        target,
    )
}

fn audit_bundle(fixture: &RelationAuditFixture) -> RelationAuditBundle<'_> {
    RelationAuditBundle {
        transition: &fixture.transition,
        report: &fixture.report,
        plan: &fixture.plan,
        evidence: fixture.evidence.as_deref(),
        assessment: &fixture.assessment,
    }
}

fn check_run(app_id: u64, expected: &CreateCheckRun) -> CheckRunRecord {
    CheckRunRecord {
        id: 42,
        name: expected.name.clone(),
        head_sha: expected.head_sha.clone(),
        external_id: Some(expected.external_id.clone()),
        status: expected.status.to_owned(),
        conclusion: Some(expected.conclusion.clone()),
        output: CheckRunOutputRecord {
            title: Some(expected.output.title.clone()),
            summary: Some(expected.output.summary.clone()),
        },
        app: Some(CheckRunApp { id: app_id }),
    }
}

struct FakeRelationRest {
    head: Result<CommitRecord, ProviderError>,
    calls: AtomicUsize,
}

impl FakeRelationRest {
    fn new(head: Result<CommitRecord, ProviderError>) -> Self {
        Self {
            head,
            calls: AtomicUsize::new(0),
        }
    }
}

impl GitHubRelationRest for FakeRelationRest {
    fn relation_head(
        &self,
        _repository: &RepositoryIdentity,
        _target: &BranchRef,
    ) -> Result<CommitRecord, ProviderError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.head.clone()
    }
}

fn oid(format: ObjectFormat, value: char) -> Oid {
    let length = match format {
        ObjectFormat::Sha1 => 40,
        ObjectFormat::Sha256 => 64,
    };
    Oid::new(format, value.to_string().repeat(length)).unwrap()
}
