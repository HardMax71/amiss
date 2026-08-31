#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed provider fixtures must fail loudly"
)]

use amiss_controller::{
    ArtifactAuditDigests, ArtifactAuditReference, ArtifactReference, IntegrationId, LeaseFence,
    PlanScope, ProviderError, RelationAuditBundle, RelationStatusRecord, RelationStatusTarget,
    RelationStatusTargets, RelationSubject, RelationSubjectHead, validate_relation_audit,
};
use amiss_controller_fixtures::relation::{RelationAuditFixture, relation_audit};
use amiss_wire::digest::{Digest, sha256};
use amiss_wire::model::{BranchRef, ObjectFormat, RepositoryIdentity};
use amiss_wire::relation::RelationVerdict;

use super::super::model::{CommitRecord, CommitStatusRecord, CreateCommitStatus, UserRecord};
use super::super::relation::{
    MARKER, StatusDecision, relation_commit_status, status_decision, validate_created,
};
use super::support::Fixture;

#[test]
fn malformed_commit_or_tree_is_not_a_finality_fact() {
    let malformed_commit = Fixture::mutated("gitea", |data| {
        data.current_head.sha = "not-an-oid".to_owned();
    });
    let malformed_tree = Fixture::mutated("forgejo", |data| {
        data.current_head.commit.tree.sha = "not-an-oid".to_owned();
    });
    for fixture in [malformed_commit, malformed_tree] {
        assert_eq!(
            fixture
                .client
                .resolve_relation_head(&subject_fixture(&fixture)),
            Err(ProviderError::InvalidResponse)
        );
    }
}

#[test]
fn subject_scope_is_rejected_before_head_resolution() {
    let fixture = Fixture::new("gitea");
    let subject = subject_fixture(&fixture);
    let mut wrong_integration = subject.clone();
    wrong_integration.scope.integration = IntegrationId::new("88".to_owned()).unwrap();
    let mut nested_owner = subject.clone();
    nested_owner.scope.repository = RepositoryIdentity::new(
        "forge.example".to_owned(),
        "group/acme".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    let mut wrong_format = subject;
    wrong_format.object_format = ObjectFormat::Sha256;

    for malformed in [wrong_integration, nested_owner, wrong_format] {
        assert_eq!(
            fixture.client.resolve_relation_head(&malformed),
            Err(ProviderError::InvalidResponse)
        );
    }
    assert!(
        fixture
            .rest
            .state
            .lock()
            .unwrap()
            .relation_requests
            .is_empty()
    );
}

#[test]
fn both_families_resolve_heads_and_publish_idempotent_statuses() {
    for namespace in ["gitea", "forgejo"] {
        let fixture = Fixture::new(namespace);
        let subject = subject_fixture(&fixture);
        assert_eq!(
            fixture.client.resolve_relation_head(&subject),
            Ok(RelationSubjectHead {
                subject: subject.clone(),
                candidate_commit: super::support::oid('b'),
            })
        );
        let (status, target) = status_fixture(&fixture);

        assert_eq!(
            fixture.client.publish_relation_status(&status, &target),
            Ok(())
        );
        assert_eq!(
            fixture.client.publish_relation_status(&status, &target),
            Ok(())
        );
        let state = fixture.rest.state.lock().unwrap();
        assert_eq!(state.created_statuses.len(), 1);
        let created = &state.created_statuses[0];
        assert_eq!(created.context, target.required_status_name);
        assert_eq!(created.state, "failure");
        assert!(created.target_url.is_empty());
        assert!(
            created
                .description
                .strip_prefix(MARKER)
                .and_then(Digest::from_wire)
                .is_some()
        );
        assert!(!created.description.contains(target.credential.as_str()));
        assert_eq!(
            state.relation_requests,
            [(subject.scope.repository, subject.target)]
        );
    }
}

#[test]
fn all_relation_verdicts_map_to_the_two_provider_states() {
    let fixture = Fixture::new("gitea");
    let (mut status, target) = status_fixture(&fixture);
    for (verdict, expected) in [
        (RelationVerdict::Aligned, "success"),
        (RelationVerdict::IntroducedDrift, "failure"),
        (RelationVerdict::PreExistingDrift, "failure"),
        (RelationVerdict::ResolvedDrift, "success"),
        (RelationVerdict::Unproven, "failure"),
    ] {
        let ArtifactAuditDigests::Relation(mut audit) = status.audit.audit else {
            panic!("the fixture carries a relation audit");
        };
        audit.verdict = verdict;
        status.audit.audit = ArtifactAuditDigests::Relation(audit);
        assert_eq!(
            relation_commit_status(&status, &target).unwrap().state,
            expected
        );
    }
}

#[test]
fn commit_status_requests_and_responses_use_the_native_wire_shape() {
    let head: CommitRecord = serde_json::from_value(serde_json::json!({
        "sha": "b".repeat(40),
        "commit": {"tree": {"sha": "d".repeat(40)}},
        "parents": [{"sha": "a".repeat(40)}]
    }))
    .unwrap();
    assert_eq!(head.sha, "b".repeat(40));
    assert_eq!(head.commit.tree.sha, "d".repeat(40));

    let decoded: CommitStatusRecord = serde_json::from_value(serde_json::json!({
        "id": 42,
        "creator": {"id": 77, "login": "amiss-controller"},
        "status": "success",
        "target_url": "",
        "description": "amiss-relation-v1: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "context": "Amiss cross-repository",
        "created_at": "2026-08-31T12:00:00Z",
        "updated_at": "2026-08-31T12:00:00Z",
        "url": "https://forge.example/api/v1/repos/acme/widget/statuses/".to_owned() + &"b".repeat(40)
    }))
    .unwrap();
    assert_eq!(decoded.id, 42);
    assert_eq!(decoded.creator.unwrap().id, 77);
    assert_eq!(decoded.status, "success");

    let request = CreateCommitStatus {
        state: "failure".to_owned(),
        target_url: String::new(),
        description: format!("{MARKER}{}", sha256(b"projection")),
        context: "Amiss cross-repository".to_owned(),
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        serde_json::json!({
            "state": "failure",
            "target_url": "",
            "description": format!("{MARKER}{}", sha256(b"projection")),
            "context": "Amiss cross-repository"
        })
    );
}

#[test]
fn a_new_owned_evaluation_advances_the_context_but_conflicts_do_not() {
    let fixture = Fixture::new("gitea");
    let (status, target) = status_fixture(&fixture);
    fixture
        .client
        .publish_relation_status(&status, &target)
        .unwrap();

    let mut newer = status.clone();
    newer.targets.fence = LeaseFence::new(8).unwrap();
    fixture
        .client
        .publish_relation_status(&newer, &target)
        .unwrap();
    assert_eq!(fixture.rest.state.lock().unwrap().created_statuses.len(), 2);

    {
        let mut state = fixture.rest.state.lock().unwrap();
        state.statuses[0].status = "success".to_owned();
    }
    assert_eq!(
        fixture.client.publish_relation_status(&newer, &target),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(fixture.rest.state.lock().unwrap().created_statuses.len(), 2);
}

#[test]
fn foreign_or_malformed_latest_contexts_are_not_overwritten() {
    let fixture = Fixture::new("forgejo");
    let (status, target) = status_fixture(&fixture);
    let expected = relation_commit_status(&status, &target).unwrap();
    let mut latest = record(&fixture, &expected);
    latest.creator = Some(UserRecord {
        id: 88,
        login: "another-writer".to_owned(),
    });
    assert!(matches!(
        status_decision(&fixture.client.config, expected.clone(), &[latest]),
        Err(ProviderError::InvalidResponse)
    ));

    let mut malformed = record(&fixture, &expected);
    malformed.description = format!("{MARKER}not-a-digest");
    assert!(matches!(
        status_decision(&fixture.client.config, expected, &[malformed]),
        Err(ProviderError::InvalidResponse)
    ));
}

#[test]
fn relation_status_shape_and_created_response_are_checked_exactly() {
    let fixture = Fixture::new("gitea");
    let (status, target) = status_fixture(&fixture);
    let expected = relation_commit_status(&status, &target).unwrap();
    assert!(matches!(
        status_decision(&fixture.client.config, expected.clone(), &[]),
        Ok(StatusDecision::Create(_))
    ));
    let created = record(&fixture, &expected);
    assert_eq!(
        validate_created(&fixture.client.config, &expected, &created),
        Ok(())
    );
    assert!(matches!(
        status_decision(&fixture.client.config, expected.clone(), &[created]),
        Ok(StatusDecision::Reuse)
    ));

    let mut completed = status.clone();
    completed.completed = true;
    let mut duplicated = status.clone();
    duplicated.targets.destinations.push(target.clone());
    let mut substituted = status;
    substituted.audit.artifact.report_digest = sha256(b"other report");
    for malformed in [completed, duplicated, substituted] {
        assert_eq!(
            fixture.client.publish_relation_status(&malformed, &target),
            Err(ProviderError::InvalidResponse)
        );
    }

    let mut wrong_response = record(&fixture, &expected);
    wrong_response.context.push_str("/other");
    assert_eq!(
        validate_created(&fixture.client.config, &expected, &wrong_response),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn a_relation_target_must_name_the_configured_reviewer_and_flat_repository() {
    let fixture = Fixture::new("gitea");
    let (status, target) = status_fixture(&fixture);
    let mut wrong_integration = target.clone();
    wrong_integration.scope.integration = IntegrationId::new("88".to_owned()).unwrap();
    let mut nested = target.clone();
    nested.scope.repository = RepositoryIdentity::new(
        "forge.example".to_owned(),
        "group/acme".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    for wrong in [wrong_integration, nested] {
        let mut wrong_status = status.clone();
        wrong_status.targets.destinations[0] = wrong.clone();
        assert_eq!(
            fixture
                .client
                .publish_relation_status(&wrong_status, &wrong),
            Err(ProviderError::InvalidResponse)
        );
    }
    assert!(
        fixture
            .rest
            .state
            .lock()
            .unwrap()
            .created_statuses
            .is_empty()
    );
}

#[test]
fn the_relation_credential_must_authenticate_as_the_dedicated_reviewer() {
    let fixture = Fixture::mutated("gitea", |data| {
        data.reviewer = UserRecord {
            id: 88,
            login: "another-writer".to_owned(),
        };
    });
    let (status, target) = status_fixture(&fixture);
    assert_eq!(
        fixture
            .client
            .resolve_relation_head(&subject_fixture(&fixture)),
        Err(ProviderError::AuthorizationRevoked)
    );
    assert_eq!(
        fixture.client.publish_relation_status(&status, &target),
        Err(ProviderError::AuthorizationRevoked)
    );
    assert!(
        fixture
            .rest
            .state
            .lock()
            .unwrap()
            .created_statuses
            .is_empty()
    );
}

fn subject_fixture(fixture: &Fixture) -> RelationSubject {
    let mut subject = relation_audit(true)
        .unwrap()
        .transition
        .relation
        .plan
        .subjects[0]
        .clone();
    subject.scope = PlanScope {
        provider: fixture.client.config.provider.clone(),
        integration: IntegrationId::new("77".to_owned()).unwrap(),
        repository: RepositoryIdentity::new(
            "forge.example".to_owned(),
            "acme".to_owned(),
            "widget".to_owned(),
        )
        .unwrap(),
    };
    subject.target = BranchRef::new("refs/heads/release/v1".to_owned()).unwrap();
    subject.object_format = ObjectFormat::Sha1;
    subject
}

fn status_fixture(fixture: &Fixture) -> (RelationStatusRecord, RelationStatusTarget) {
    let audit_fixture = relation_audit(true).unwrap();
    let source = &audit_fixture.transition.relation.plan.subjects[0];
    let target = RelationStatusTarget {
        role: source.role.clone(),
        scope: PlanScope {
            provider: fixture.client.config.provider.clone(),
            integration: IntegrationId::new("77".to_owned()).unwrap(),
            repository: RepositoryIdentity::new(
                "forge.example".to_owned(),
                "acme".to_owned(),
                "widget".to_owned(),
            )
            .unwrap(),
        },
        credential: source.credential.clone(),
        candidate_commit: audit_fixture.transition.subjects[0]
            .commits
            .candidate
            .clone(),
        required_status_name: "Amiss cross-repository".to_owned(),
    };
    let audit = validate_relation_audit(audit_bundle(&audit_fixture)).unwrap();
    (
        RelationStatusRecord {
            targets: RelationStatusTargets {
                relation: audit_fixture.transition.relation.plan.identity.clone(),
                coordination: audit_fixture.transition.coordination.clone(),
                trigger_role: audit_fixture.transition.relation.trigger_role.clone(),
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
        },
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

fn record(fixture: &Fixture, expected: &CreateCommitStatus) -> CommitStatusRecord {
    CommitStatusRecord {
        id: 42,
        creator: Some(UserRecord {
            id: fixture.client.config.reviewer.id,
            login: fixture.client.config.reviewer.login.clone(),
        }),
        status: expected.state.clone(),
        target_url: expected.target_url.clone(),
        description: expected.description.clone(),
        context: expected.context.clone(),
    }
}
