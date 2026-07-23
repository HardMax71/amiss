#![expect(
    clippy::unwrap_used,
    reason = "fixed provider records and identities must fail loudly"
)]

mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use amiss_controller::{
    ChangeState, CheckConclusion, HandleOutcome, OpaqueId, ProviderAdapter, ProviderError,
    RunFailure,
};
use amiss_controller_gitlab::{
    GitLabAccess, GitLabApi, GitLabMergeTrainAdapter, GitLabProtection, GitLabRefresh,
    GitLabRefreshQuery, policy_job_accepted,
};

use support::identity::now_seconds;
use support::oidc::{accept, claims, oidc};
use support::refresh::{publication, valid_refresh};

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

#[test]
fn wrong_job_pipeline_and_commit_topology_are_invalid_provider_data() {
    let (source, delivery, valid) = fixture();
    let mut cases = Vec::new();
    let mut project_job = valid.clone();
    project_job.job.source = "project".to_owned();
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
fn only_a_published_pass_can_succeed_the_policy_job() {
    let evaluation_id = OpaqueId::new("evaluation/1".to_owned()).unwrap();
    assert!(policy_job_accepted(&HandleOutcome::Published(
        CheckConclusion::Pass
    )));
    for outcome in [
        HandleOutcome::Published(CheckConclusion::Block),
        HandleOutcome::Published(CheckConclusion::Superseded),
        HandleOutcome::Published(CheckConclusion::Unavailable(RunFailure::Unavailable)),
        HandleOutcome::Duplicate {
            evaluation_id: evaluation_id.clone(),
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
