#![expect(
    clippy::unwrap_used,
    reason = "fixed provider records and identities must fail loudly"
)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use amiss_controller::{
    ArtifactAuditDigests, ArtifactAuditReference, ArtifactReference, ChangeId, ChangeState,
    CheckConclusion, HandleOutcome, LeaseFence, OpaqueId, PlanScope, ProviderAdapter,
    ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt,
    RelationAuditDigests, RelationLimits, RelationStatusRecord, RelationStatusTarget,
    RelationStatusTargets, RelationSubject, RunFailure,
};
use amiss_controller_gitlab::{
    GitLabAccess, GitLabApi, GitLabMergeTrainAdapter, GitLabProtection, GitLabRefresh,
    GitLabRefreshQuery, policy_job_accepted,
};

use amiss_wire::controls::{ProjectionSource, RecordSetSelection};
use amiss_wire::digest::sha256;
use amiss_wire::model::{ArtifactId, BranchRef, ObjectFormat, RepositoryIdentity};
use amiss_wire::relation::RelationVerdict;

use crate::support::identity::{HOST, now_seconds};
use crate::support::oidc::{accept, claims, oidc};
use crate::support::refresh::{publication, valid_refresh};

const BODY: &[u8] = br#"{"merge_request_iid":42}"#;

#[derive(Clone)]
struct FakeApi {
    state: Arc<Mutex<FakeState>>,
}

struct FakeState {
    responses: VecDeque<GitLabRefresh>,
    queries: Vec<GitLabRefreshQuery>,
}

impl FakeApi {
    fn new(responses: impl IntoIterator<Item = GitLabRefresh>) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                responses: responses.into_iter().collect(),
                queries: Vec::new(),
            })),
        }
    }
}

impl GitLabApi for FakeApi {
    fn refresh(&self, query: &GitLabRefreshQuery) -> Result<GitLabRefresh, ProviderError> {
        let mut state = self.state.lock().unwrap();
        state.queries.push(query.clone());
        if state.responses.len() > 1 {
            state
                .responses
                .pop_front()
                .ok_or(ProviderError::Unavailable)
        } else {
            state
                .responses
                .front()
                .cloned()
                .ok_or(ProviderError::Unavailable)
        }
    }
}

#[test]
fn active_snapshot_is_the_exact_train_commit_and_first_parent() {
    let now = now_seconds();
    let source = oidc();
    let delivery = accept(&source, &claims(now), BODY, now)
        .unwrap()
        .delivery()
        .clone();
    let api = FakeApi::new([valid_refresh(&delivery)]);
    let adapter = GitLabMergeTrainAdapter::new(source, api.clone());
    let snapshot = adapter.refresh(&delivery).unwrap();

    assert_eq!(snapshot.state, ChangeState::Active);
    assert_eq!(snapshot.run.commits.base.as_str(), "a".repeat(40));
    assert_eq!(snapshot.run.commits.candidate.as_str(), "b".repeat(40));
    assert_eq!(snapshot.run.trees.base.as_str(), "f".repeat(40));
    assert_eq!(snapshot.run.trees.candidate.as_str(), "e".repeat(40));
    assert_eq!(snapshot.gate_commit, snapshot.run.commits.candidate);
    let queries = api.state.lock().unwrap().queries.clone();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].project_id, 101);
    assert_eq!(queries[0].merge_request_iid, 42);
    assert_eq!(queries[0].pipeline_id, 202);
    assert_eq!(queries[0].job_id, 303);
    assert_eq!(queries[0].runner_id, 77);
}

/// gitlab.com answers the jobs API with a null source while the OIDC
/// `job_source` claim it signed already named the policy, so an absent REST
/// source stays active and only a wrong one is invalid.
#[test]
fn an_absent_rest_job_source_still_names_the_policy_job() {
    let now = now_seconds();
    let source = oidc();
    let delivery = accept(&source, &claims(now), BODY, now)
        .unwrap()
        .delivery()
        .clone();
    let mut refresh = valid_refresh(&delivery);
    refresh.job.source = None;
    let api = FakeApi::new([refresh]);
    let adapter = GitLabMergeTrainAdapter::new(source, api);
    let snapshot = adapter.refresh(&delivery).unwrap();
    assert_eq!(snapshot.state, ChangeState::Active);
}

#[test]
fn wrong_job_pipeline_and_commit_topology_are_invalid_provider_data() {
    let (source, delivery, valid) = fixture();
    let mut cases = Vec::new();
    let mut project_job = valid.clone();
    project_job.job.source = Some("project".to_owned());
    cases.push(project_job);
    let mut wrong_pipeline = valid.clone();
    wrong_pipeline.pipeline.sha = "d".repeat(40);
    cases.push(wrong_pipeline);
    let mut wrong_runner = valid.clone();
    wrong_runner.job.runner_id = 88;
    cases.push(wrong_runner);
    let mut wrong_source_parent = valid.clone();
    wrong_source_parent.gate.parents = vec!["a".repeat(40), "d".repeat(40)];
    cases.push(wrong_source_parent);
    let mut extra_parent = valid.clone();
    extra_parent.gate.parents.push("d".repeat(40));
    cases.push(extra_parent);
    let mut wrong_project = valid;
    wrong_project.project.http_url_to_repo = "https://gitlab.example/acme/other.git".to_owned();
    cases.push(wrong_project);

    for refresh in cases {
        let adapter = GitLabMergeTrainAdapter::new(Arc::clone(&source), FakeApi::new([refresh]));
        assert_eq!(
            adapter.refresh(&delivery),
            Err(ProviderError::InvalidResponse)
        );
    }
}

/// The refresh query binds the delivery to the policy on eight separate
/// clauses, so each one is broken alone and the run has to refuse.
#[test]
fn every_binding_clause_of_the_refresh_query_stands_alone() {
    let (source, delivery, valid) = fixture();
    let elsewhere = ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("other.example".to_owned()).unwrap(),
    };

    let mut foreign_identity = delivery.clone();
    foreign_identity.identity.provider = elsewhere.clone();
    let mut foreign_change = delivery.clone();
    foreign_change.change.provider = elsewhere;
    let mut other_integration = delivery.clone();
    other_integration.identity.integration = OpaqueId::new("policy/2".to_owned()).unwrap();
    let mut other_repository = delivery.clone();
    other_repository.change.repository =
        RepositoryIdentity::new(HOST.to_owned(), "acme".to_owned(), "other".to_owned()).unwrap();
    let mut other_project = delivery.clone();
    other_project.change.change = ChangeId::new("project/102/merge-request/42".to_owned()).unwrap();
    let mut retried = delivery.clone();
    retried.provider_run.attempt = ProviderRunAttempt::new(2).unwrap();
    let mut wider_format = delivery.clone();
    wider_format.provider_run.object_format = ObjectFormat::Sha256;

    for broken in [
        foreign_identity,
        foreign_change,
        other_integration,
        other_repository,
        other_project,
        retried,
        wider_format,
    ] {
        let adapter =
            GitLabMergeTrainAdapter::new(Arc::clone(&source), FakeApi::new([valid.clone()]));
        assert_eq!(
            adapter.refresh(&broken),
            Err(ProviderError::InvalidResponse),
            "{broken:?}"
        );
    }
}

#[test]
fn stale_train_and_closed_change_do_not_run() {
    let (source, delivery, valid) = fixture();
    let mut stale = valid.clone();
    stale.train.as_mut().unwrap().status = "stale".to_owned();
    let mut missing = valid.clone();
    missing.train = None;
    let mut draft = valid.clone();
    draft.merge_request.draft = true;
    let mut stopped = valid.clone();
    stopped.pipeline.status = "failed".to_owned();
    for refresh in [stale, missing, draft, stopped] {
        let adapter = GitLabMergeTrainAdapter::new(Arc::clone(&source), FakeApi::new([refresh]));
        assert_eq!(
            adapter.refresh(&delivery).unwrap().state,
            ChangeState::Superseded
        );
    }
    let mut closed = valid;
    closed.merge_request.state = "closed".to_owned();
    let adapter = GitLabMergeTrainAdapter::new(source, FakeApi::new([closed]));
    assert_eq!(
        adapter.refresh(&delivery).unwrap().state,
        ChangeState::Closed
    );
}

#[test]
fn every_merge_and_protection_bypass_revokes_authorization() {
    let (source, delivery, valid) = fixture();
    let mut cases = Vec::new();
    for method in ["ff", "rebase_merge"] {
        let mut refresh = valid.clone();
        refresh.project.merge_method = method.to_owned();
        cases.push(refresh);
    }
    let mut bypass = valid.clone();
    bypass.project.train.enforcement = "allow_bypass".to_owned();
    cases.push(bypass);
    let mut skip_train = valid.clone();
    skip_train.project.train.skip_allowed = true;
    cases.push(skip_train);
    let mut skipped_pipeline = valid.clone();
    skipped_pipeline.project.checks.skipped_pipeline_allowed = true;
    cases.push(skipped_pipeline);
    let mut squash = valid.clone();
    squash.merge_request.squash_on_merge = true;
    cases.push(squash);
    let mut force = valid.clone();
    force.protections[0].allow_force_push = true;
    cases.push(force);
    let mut role = valid.clone();
    role.protections[0].push_access_levels[0].member_role_id = Some(9);
    cases.push(role);
    let mut group = valid.clone();
    group.protections[0].push_access_levels[0].group_id = Some(9);
    cases.push(group);
    let mut deploy_key = valid.clone();
    deploy_key.protections[0].push_access_levels[0].deploy_key_id = Some(9);
    cases.push(deploy_key);
    let mut direct_push = valid.clone();
    direct_push.protections[0].push_access_levels[0].access_level = 40;
    cases.push(direct_push);
    let mut unprotected = valid.clone();
    unprotected.protections.clear();
    cases.push(unprotected);
    let mut permissive_wildcard = valid;
    permissive_wildcard.protections.push(GitLabProtection {
        name: "*".to_owned(),
        allow_force_push: false,
        push_access_levels: vec![GitLabAccess {
            access_level: 30,
            user_id: None,
            group_id: None,
            deploy_key_id: None,
            member_role_id: None,
        }],
    });
    cases.push(permissive_wildcard);

    for refresh in cases {
        let adapter = GitLabMergeTrainAdapter::new(Arc::clone(&source), FakeApi::new([refresh]));
        assert_eq!(
            adapter.refresh(&delivery).unwrap().state,
            ChangeState::AuthorizationRevoked
        );
    }
}

#[test]
fn publication_performs_a_final_authoritative_refresh() {
    let (source, delivery, valid) = fixture();
    let exact_api = FakeApi::new([valid.clone()]);
    let exact = GitLabMergeTrainAdapter::new(Arc::clone(&source), exact_api);
    let snapshot = exact.refresh(&delivery).unwrap();
    let pass = publication(&delivery, &snapshot, CheckConclusion::Pass);
    assert_eq!(exact.publish(&delivery, &pass), Ok(()));

    let mut stale = valid;
    stale.train.as_mut().unwrap().status = "stale".to_owned();
    let drifted = GitLabMergeTrainAdapter::new(source, FakeApi::new([stale]));
    assert_eq!(
        drifted.publish(&delivery, &pass),
        Err(ProviderError::AuthorizationRevoked)
    );
    let mut wrong_run = pass;
    wrong_run.provider_run.run_id = OpaqueId::new("pipeline/999/job/303".to_owned()).unwrap();
    assert_eq!(
        drifted.publish(&delivery, &wrong_run),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn policy_job_resolves_only_its_ephemeral_relation_candidate() {
    let (source, delivery, valid) = fixture();
    let api = FakeApi::new([valid]);
    let adapter = GitLabMergeTrainAdapter::new(source, api.clone());
    let mut subject = RelationSubject {
        role: ArtifactId::new("source".to_owned()).unwrap(),
        scope: PlanScope {
            provider: delivery.identity.provider.clone(),
            integration: delivery.identity.integration.clone(),
            repository: delivery.change.repository.clone(),
        },
        target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        object_format: ObjectFormat::Sha1,
        credential: OpaqueId::new("credential/gitlab".to_owned()).unwrap(),
        source: ProjectionSource::RecordSet(RecordSetSelection {
            set: ArtifactId::new("rust/public-api".to_owned()).unwrap(),
        }),
        limits: RelationLimits {
            acquisition_objects: 100,
            acquisition_bytes: 1_048_576,
            projection_records: 100,
            projection_bytes: 1_048_576,
        },
    };

    assert_eq!(
        adapter.resolve_relation_head(&delivery, &subject),
        Ok(amiss_controller::RelationSubjectHead {
            subject: subject.clone(),
            candidate_commit: delivery.provider_run.candidate_commit.clone(),
        })
    );
    subject.target = BranchRef::new("refs/heads/other".to_owned()).unwrap();
    assert_eq!(
        adapter.resolve_relation_head(&delivery, &subject),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(api.state.lock().unwrap().queries.len(), 1);
}

#[test]
fn relation_result_is_the_exact_live_policy_job_decision() {
    let (source, delivery, valid) = fixture();
    for (verdict, expected) in [
        (RelationVerdict::Aligned, true),
        (RelationVerdict::IntroducedDrift, false),
    ] {
        let (status, target) = relation_status(&delivery, verdict);
        let adapter =
            GitLabMergeTrainAdapter::new(Arc::clone(&source), FakeApi::new([valid.clone()]));
        assert_eq!(
            adapter.relation_policy_job_result(&delivery, &status, &target),
            Ok(expected)
        );
    }

    let (status, target) = relation_status(&delivery, RelationVerdict::Aligned);
    let mut stopped = valid;
    stopped.job.status = "failed".to_owned();
    let adapter = GitLabMergeTrainAdapter::new(source, FakeApi::new([stopped]));
    assert_eq!(
        adapter.relation_policy_job_result(&delivery, &status, &target),
        Err(ProviderError::AuthorizationRevoked)
    );
}

#[test]
fn malformed_relation_bindings_are_rejected_before_provider_io() {
    let (source, delivery, valid) = fixture();
    let api = FakeApi::new([valid]);
    let adapter = GitLabMergeTrainAdapter::new(source, api.clone());
    let (status, mut target) = relation_status(&delivery, RelationVerdict::Aligned);
    target.required_status_name = "another-job".to_owned();
    assert_eq!(
        adapter.relation_policy_job_result(&delivery, &status, &target),
        Err(ProviderError::InvalidResponse)
    );

    assert!(api.state.lock().unwrap().queries.is_empty());
}

#[test]
fn only_a_published_pass_can_succeed_the_policy_job() {
    let evaluation_id = OpaqueId::new("evaluation/1".to_owned()).unwrap();
    assert!(policy_job_accepted(&HandleOutcome::Published {
        conclusion: CheckConclusion::Pass,
        artifact: None,
    }));
    for outcome in [
        HandleOutcome::Published {
            conclusion: CheckConclusion::Block,
            artifact: None,
        },
        HandleOutcome::Published {
            conclusion: CheckConclusion::Superseded,
            artifact: None,
        },
        HandleOutcome::Published {
            conclusion: CheckConclusion::Unavailable(RunFailure::Unavailable),
            artifact: None,
        },
        HandleOutcome::Duplicate {
            evaluation_id: evaluation_id.clone(),
            artifact: None,
        },
        HandleOutcome::InProgress {
            evaluation_id,
            retry_at_unix_millis: 1,
        },
    ] {
        assert!(!policy_job_accepted(&outcome));
    }
}

fn fixture() -> (
    Arc<amiss_controller_gitlab::GitLabOidc>,
    amiss_controller::AuthenticatedDelivery,
    GitLabRefresh,
) {
    let now = now_seconds();
    let source = oidc();
    let delivery = accept(&source, &claims(now), BODY, now)
        .unwrap()
        .delivery()
        .clone();
    let refresh = valid_refresh(&delivery);
    (source, delivery, refresh)
}

fn relation_status(
    delivery: &amiss_controller::AuthenticatedDelivery,
    verdict: RelationVerdict,
) -> (RelationStatusRecord, RelationStatusTarget) {
    let report_digest = sha256(b"report");
    let target = RelationStatusTarget {
        role: ArtifactId::new("source".to_owned()).unwrap(),
        scope: PlanScope {
            provider: delivery.identity.provider.clone(),
            integration: delivery.identity.integration.clone(),
            repository: delivery.change.repository.clone(),
        },
        credential: OpaqueId::new("credential/gitlab".to_owned()).unwrap(),
        candidate_commit: delivery.provider_run.candidate_commit.clone(),
        required_status_name: "amiss:policy".to_owned(),
    };
    (
        RelationStatusRecord {
            targets: RelationStatusTargets {
                relation: ArtifactId::new("relation/public-api".to_owned()).unwrap(),
                coordination: ArtifactId::new("workflow/release-42".to_owned()).unwrap(),
                trigger_role: ArtifactId::new("source".to_owned()).unwrap(),
                fence: LeaseFence::new(1).unwrap(),
                destinations: vec![target.clone()],
            },
            audit: ArtifactAuditReference {
                artifact: ArtifactReference {
                    id: "1".repeat(64),
                    locator: format!("https://amiss.example/artifacts/{}/report", "1".repeat(64)),
                    expires_at_unix_millis: 1_800_000_000_000,
                    report_digest,
                    semantic_digest: None,
                    assessment_digest: None,
                    external_tally: None,
                    external_incomplete: false,
                },
                audit: ArtifactAuditDigests::Relation(RelationAuditDigests {
                    report_digest,
                    plan_digest: sha256(b"plan"),
                    evidence_digest: Some(sha256(b"evidence")),
                    assessment_digest: sha256(b"assessment"),
                    verdict,
                }),
            },
            completed: false,
        },
        target,
    )
}
