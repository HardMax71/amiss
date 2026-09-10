#![expect(
    clippy::unwrap_used,
    reason = "fixed provider payloads and protocol identities must fail loudly"
)]

mod check_runs;
mod check_suites;
mod review_comments;
mod review_threads;
mod reviews;

use amiss_controller_fixtures::clock::TestClock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, CheckBinding, CheckConclusion,
    ControllerEvaluationId, DeliveryHeader, DeliveryRoute, GitHubWebhook, IngressCheck,
    IngressLimits, IngressPolicy, OidPair, OpaqueId, ProviderAdapter, ProviderError,
    ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt, Publication,
    ReplayWindow, RunIdentity, RunRefs, SemanticEvidenceExpectation, SignedTimePolicy,
    UntrustedDelivery, WebhookKey, WebhookKeyring, WorkflowArtifactExpectation,
};
use amiss_controller_github::check::CheckRunStatus;
use amiss_controller_github::repository::metadata::RepositoryOrganization;
use amiss_controller_github::repository::pull::PullRepositoryRecord;
use amiss_controller_github::webhook::event::GitHubEvent;
use amiss_controller_github::webhook::pull::request::{PullRequestWebhook, SynchronizePullRequest};
use amiss_controller_github::webhook::{
    BaseChange, GitHubPayload, Installation, PreviousReference, PullRequestChanges, Workflow,
    WorkflowRun, WorkflowRunConclusion,
};
use amiss_controller_github::workflow::{
    WorkflowPullRef, WorkflowPullRepository, WorkflowPullRequest,
};
use amiss_controller_github::{
    GitHubApi, GitHubPullRequest, GitHubPullRequestAdapter, GitHubPullRequestSource,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{
    ArtifactId, BranchRef, ForgeDialect, ObjectFormat, Oid, RepoPathText, RepositoryIdentity,
};
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;

const NOW: i64 = 1_800_000_000_000;
const SECRET: &[u8] = b"github-webhook-secret";
static BODY: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut repository: PullRepositoryRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY).unwrap();
    repository.id = 101;
    "widget".clone_into(&mut repository.name);
    "HardMax71/widget".clone_into(&mut repository.full_name);
    "HardMax71".clone_into(&mut repository.owner.login);
    let mut pull: PullRequestWebhook =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL).unwrap();
    pull.request.id = 4_201;
    pull.request.number = 42;
    pull.request.head.sha = oid('b');
    "topic".clone_into(&mut pull.request.head.branch);
    "main".clone_into(&mut pull.request.base.branch);
    pull.request.base.repo = Some(repository.clone());
    serde_json::to_vec(&GitHubPayload {
        action: Some("opened".to_owned()),
        changes: None,
        installation: Some(Installation {
            id: 7,
            node_id: "installation-seven".to_owned(),
        }),
        repository: Some(repository),
        number: Some(42),
        pull_request: Some(pull),
        review: None,
        comment: None,
        thread: None,
        check_suite: None,
        check_run: None,
        workflow: None,
        workflow_run: None,
    })
    .unwrap()
});
type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
struct FakeApi {
    state: Arc<FakeApiState>,
}

struct FakeApiState {
    snapshot: Mutex<ChangeSnapshot>,
    refreshes: AtomicUsize,
    requests: Mutex<Vec<ApiRequest>>,
    publications: Mutex<Vec<Publication>>,
}

#[derive(Debug, PartialEq, Eq)]
struct ApiRequest {
    installation_id: u64,
    repository_id: u64,
    owner: String,
    name: String,
    pull_request_id: u64,
    number: u64,
    candidate: String,
}

impl FakeApi {
    fn new(snapshot: ChangeSnapshot) -> Self {
        Self {
            state: Arc::new(FakeApiState {
                snapshot: Mutex::new(snapshot),
                refreshes: AtomicUsize::new(0),
                requests: Mutex::new(Vec::new()),
                publications: Mutex::new(Vec::new()),
            }),
        }
    }
}

impl GitHubApi for FakeApi {
    fn refresh(
        &self,
        pull_request: GitHubPullRequest<'_>,
    ) -> Result<ChangeSnapshot, ProviderError> {
        self.state.refreshes.fetch_add(1, Ordering::Relaxed);
        self.state
            .requests
            .lock()
            .unwrap()
            .push(observed(pull_request));
        Ok(self.state.snapshot.lock().unwrap().clone())
    }

    fn publish(
        &self,
        pull_request: GitHubPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.state
            .requests
            .lock()
            .unwrap()
            .push(observed(pull_request));
        self.state
            .publications
            .lock()
            .unwrap()
            .push(publication.clone());
        Ok(())
    }
}

struct DropApi {
    dropped: Arc<AtomicBool>,
}

impl Drop for DropApi {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::Release);
    }
}

impl GitHubApi for DropApi {
    fn refresh(
        &self,
        _pull_request: GitHubPullRequest<'_>,
    ) -> Result<ChangeSnapshot, ProviderError> {
        Err(ProviderError::Unavailable)
    }

    fn publish(
        &self,
        _pull_request: GitHubPullRequest<'_>,
        _publication: &Publication,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Unavailable)
    }
}

trait SignedSource {
    fn authenticate_delivery(
        &self,
        check: IngressCheck<'_>,
    ) -> Result<amiss_controller::VerifiedDelivery, ProviderError>;
}

impl<A: GitHubApi> SignedSource for GitHubPullRequestAdapter<A> {
    fn authenticate_delivery(
        &self,
        check: IngressCheck<'_>,
    ) -> Result<amiss_controller::VerifiedDelivery, ProviderError> {
        ProviderAdapter::authenticate(self, check)
    }
}

impl SignedSource for GitHubPullRequestSource {
    fn authenticate_delivery(
        &self,
        check: IngressCheck<'_>,
    ) -> Result<amiss_controller::VerifiedDelivery, ProviderError> {
        Self::authenticate(self, check)
    }
}

#[test]
fn signed_body_alone_defines_the_pull_request() {
    let adapter = adapter(FakeApi::new(dummy_snapshot()));
    let first = authenticated(
        &adapter,
        &BODY,
        &[
            ("x-github-event", b"issues"),
            ("x-github-delivery", b"forged-one"),
        ],
        SignedTimePolicy::ReplayOnly,
        provider(),
    )
    .unwrap();
    let pretty = authenticated(
        &adapter,
        &BODY,
        &[
            ("x-github-event", b"push"),
            ("x-github-delivery", b"forged-two"),
        ],
        SignedTimePolicy::ReplayOnly,
        provider(),
    )
    .unwrap();

    assert_eq!(first.delivery().identity.integration.as_str(), "7");
    assert_eq!(first.delivery().change.repository.owner(), "hardmax71");
    assert_eq!(first.delivery().change.repository.name(), "widget");
    assert_eq!(
        first.delivery().change.change.as_str(),
        "repository/101/pull/4201/number/42"
    );
    assert_eq!(
        first.delivery().provider_run.candidate_commit.as_str(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(
        first.delivery().provider_run,
        pretty.delivery().provider_run
    );

    for action in ["reopened", "synchronize"] {
        let body = replaced_once(
            &BODY,
            r#""action":"opened""#,
            &format!(r#""action":"{action}""#),
        );
        let delivery = authenticated(
            &adapter,
            &body,
            &[],
            SignedTimePolicy::ReplayOnly,
            provider(),
        )
        .unwrap();
        assert_eq!(
            delivery.delivery().provider_run,
            first.delivery().provider_run
        );
    }
}

#[test]
fn signed_target_must_belong_to_the_configured_lane() {
    let source = source();
    let main = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let release = BranchRef::new("refs/heads/release".to_owned()).unwrap();

    assert!(matches!(
        authenticate_target(&source, &BODY, &main),
        Ok(Some(_))
    ));
    assert_eq!(
        authenticate_target(&source, &BODY, &release),
        Err(ProviderError::AuthorizationRevoked)
    );
}

#[test]
fn configured_workflow_completion_reproduces_the_pull_request_run() {
    let completion_source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let pull_request = authenticate_target(&source(), &BODY, &target)
        .unwrap()
        .unwrap();
    let body = serde_json::to_vec(&workflow_payload()).unwrap();
    let completion = authenticate_target(&completion_source, &body, &target)
        .unwrap()
        .unwrap();

    assert_eq!(completion.delivery().change, pull_request.delivery().change);
    assert_eq!(
        completion.delivery().provider_run,
        pull_request.delivery().provider_run
    );
    assert_eq!(
        authenticate_target(&completion_source, &BODY, &target),
        Ok(None)
    );

    let numeric = GitHubPullRequestSource::new(provider(), webhook(), &[workflow_artifact("321")]);
    assert!(matches!(
        authenticate_target(&numeric, &body, &target),
        Ok(Some(_))
    ));
}

#[test]
fn only_a_successful_configured_completion_with_one_pull_request_is_work() {
    let source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let changes: [fn(&mut GitHubPayload); 4] = [
        |p| p.action = Some("in_progress".to_owned()),
        |p| p.workflow.as_mut().unwrap().path = ".github/workflows/other.yml".to_owned(),
        |p| p.workflow_run.as_mut().unwrap().conclusion = Some(WorkflowRunConclusion::Failure),
        |p| p.workflow_run.as_mut().unwrap().pull_requests.clear(),
    ];
    for (index, change) in changes.into_iter().enumerate() {
        let mut payload = workflow_payload();
        change(&mut payload);
        let body = serde_json::to_vec(&payload).unwrap();
        assert_eq!(
            authenticate_target(&source, &body, &target),
            Ok(None),
            "change {index}"
        );
    }
    let check_run = amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN;
    assert_eq!(authenticate_target(&source, check_run, &target), Ok(None));

    let other_target = BranchRef::new("refs/heads/release".to_owned()).unwrap();
    let body = serde_json::to_vec(&workflow_payload()).unwrap();
    assert_eq!(
        authenticate_target(&source, &body, &other_target),
        Err(ProviderError::AuthorizationRevoked)
    );
}

#[test]
fn signed_workflow_repositories_reject_unknown_metadata() {
    let source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let payload = workflow_payload();
    let run = payload.workflow_run.as_ref().unwrap();
    let body = serde_json::to_string(&payload).unwrap();
    for repository in [
        serde_json::to_string(payload.repository.as_ref().unwrap()).unwrap(),
        serde_json::to_string(&run.repository).unwrap(),
        serde_json::to_string(&run.head_repository).unwrap(),
    ] {
        assert_eq!(body.matches(&repository).count(), 1);
        for invalid in [
            repository.replacen('{', r#"{"unknown":true,"#, 1),
            repository.replacen(r#""owner":{"#, r#""owner":{"unknown":true,"#, 1),
        ] {
            assert_ne!(invalid, repository);
            let changed = body.replacen(&repository, &invalid, 1);
            assert_eq!(
                authenticate_target(&source, changed.as_bytes(), &target),
                Err(ProviderError::Authentication)
            );
        }
    }
}

#[test]
fn signed_workflow_head_commit_is_a_typed_record() {
    let source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let payload = workflow_payload();
    let commit =
        serde_json::to_string(&payload.workflow_run.as_ref().unwrap().head_commit).unwrap();
    let body = serde_json::to_string(&payload).unwrap();
    assert_eq!(body.matches(&commit).count(), 1);
    let original = authenticate_target(&source, body.as_bytes(), &target)
        .unwrap()
        .unwrap();
    for replacement in [
        "false".to_owned(),
        "null".to_owned(),
        "{}".to_owned(),
        commit.replacen('{', r#"{"unknown":true,"#, 1),
        commit.replacen(r#""author":{"#, r#""author":{"unknown":true,"#, 1),
        commit.replacen(r#""committer":{"#, r#""committer":{"unknown":true,"#, 1),
    ] {
        assert_ne!(replacement, commit);
        let invalid = body.replacen(&commit, &replacement, 1);
        assert_eq!(
            authenticate_target(&source, invalid.as_bytes(), &target),
            Err(ProviderError::Authentication)
        );
    }
    let member = format!("\"head_commit\":{commit},");
    assert_eq!(body.matches(&member).count(), 1);
    let missing = body.replacen(&member, "", 1);
    assert_eq!(
        authenticate_target(&source, missing.as_bytes(), &target),
        Err(ProviderError::Authentication)
    );

    let mut metadata = payload.clone();
    let commit = &mut metadata.workflow_run.as_mut().unwrap().head_commit;
    for author in [&mut commit.author, &mut commit.committer] {
        author.email = amiss_wire::assessment::Nullable::Null;
        author.date = Some("2020-10-05T16:32:07Z".to_owned());
        author.username = Some("fixture-author".to_owned());
    }
    commit.message = "Changed metadata".to_owned();
    commit.tree_id = oid('f');
    let input = serde_json::to_vec(&metadata).unwrap();
    let accepted = authenticate_target(&source, &input, &target)
        .unwrap()
        .unwrap();
    assert_eq!(accepted.delivery(), original.delivery());
}

#[test]
fn signed_workflow_run_keeps_its_closed_contract() {
    let source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let payload = workflow_payload();
    let body = serde_json::to_string(&payload).unwrap();
    let original = authenticate_target(&source, body.as_bytes(), &target)
        .unwrap()
        .unwrap();
    for (old, new) in [
        (r#""workflow_run":{"#, r#""workflow_run":{"unknown":true,"#),
        (r#""actor":{"#, r#""actor":{"unknown":true,"#),
        (
            r#""triggering_actor":{"#,
            r#""triggering_actor":{"unknown":true,"#,
        ),
        (r#""conclusion":"success","#, ""),
        (r#""run_attempt":2"#, r#""run_attempt":9007199254740992"#),
        (r#""status":"completed""#, r#""status":{"completed":null}"#),
        (
            r#""conclusion":"success""#,
            r#""conclusion":{"success":null}"#,
        ),
    ] {
        assert_eq!(body.matches(old).count(), 1, "{old}");
        let invalid = body.replacen(old, new, 1);
        assert_eq!(
            authenticate_target(&source, invalid.as_bytes(), &target),
            Err(ProviderError::Authentication),
            "{old}"
        );
    }
    let mut metadata = payload.clone();
    let run = metadata.workflow_run.as_mut().unwrap();
    run.actor = amiss_wire::assessment::Nullable::Null;
    run.triggering_actor = amiss_wire::assessment::Nullable::Null;
    run.name = amiss_wire::assessment::Nullable::Null;
    run.display_title = None;
    run.referenced_workflows = Some(amiss_wire::assessment::Nullable::Null);
    let accepted = authenticate_target(&source, &serde_json::to_vec(&metadata).unwrap(), &target)
        .unwrap()
        .unwrap();
    assert_eq!(accepted.delivery(), original.delivery());

    for status in [
        CheckRunStatus::Queued,
        CheckRunStatus::InProgress,
        CheckRunStatus::Waiting,
        CheckRunStatus::Requested,
        CheckRunStatus::Pending,
    ] {
        metadata.workflow_run.as_mut().unwrap().status = status;
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&metadata).unwrap(), &target),
            Err(ProviderError::Authentication)
        );
    }
    for conclusion in [
        None,
        Some(WorkflowRunConclusion::ActionRequired),
        Some(WorkflowRunConclusion::Cancelled),
        Some(WorkflowRunConclusion::Failure),
        Some(WorkflowRunConclusion::Neutral),
        Some(WorkflowRunConclusion::Skipped),
        Some(WorkflowRunConclusion::Stale),
        Some(WorkflowRunConclusion::TimedOut),
        Some(WorkflowRunConclusion::StartupFailure),
    ] {
        let mut candidate = payload.clone();
        candidate.workflow_run.as_mut().unwrap().conclusion = conclusion;
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&candidate).unwrap(), &target),
            Ok(None)
        );
    }
    let pr = payload.workflow_run.as_ref().unwrap().pull_requests[0].clone();
    for requests in [
        vec![],
        vec![None],
        vec![pr.clone(), pr.clone()],
        vec![None, pr],
    ] {
        let mut candidate = payload.clone();
        candidate.workflow_run.as_mut().unwrap().pull_requests = requests;
        assert_eq!(
            authenticate_target(&source, &serde_json::to_vec(&candidate).unwrap(), &target),
            Ok(None)
        );
    }
}

#[test]
fn workflow_repository_identity_is_independent_of_retained_metadata() {
    let source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let payload = workflow_payload();
    let original = authenticate_target(&source, &serde_json::to_vec(&payload).unwrap(), &target)
        .unwrap()
        .unwrap();
    let mut metadata = payload.clone();
    let run = metadata.workflow_run.as_mut().unwrap();
    run.repository.node_id = "distinct-root-metadata".to_owned();
    run.repository.owner.as_mut().unwrap().name = Some("Display name".to_owned());
    run.head_repository.owner.as_mut().unwrap().email =
        Some(amiss_wire::assessment::Nullable::Null);
    let accepted = authenticate_target(&source, &serde_json::to_vec(&metadata).unwrap(), &target)
        .unwrap()
        .unwrap();
    assert_eq!(accepted.delivery(), original.delivery());

    let mutations: [fn(&mut WorkflowRun); 8] = [
        |run| run.repository.id += 1,
        |run| run.repository.name = "other".to_owned(),
        |run| run.repository.full_name = "other/widget".to_owned(),
        |run| run.repository.owner.as_mut().unwrap().login = "other".to_owned(),
        |run| run.repository.owner = None,
        |run| run.head_repository.owner = None,
        |run| run.head_repository.owner.as_mut().unwrap().login = "other".to_owned(),
        |run| {
            run.head_repository.owner.as_mut().unwrap().login = "bad name".to_owned();
            run.head_repository.full_name = "bad name/widget".to_owned();
        },
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut candidate = payload.clone();
        mutate(candidate.workflow_run.as_mut().unwrap());
        let input = serde_json::to_vec(&candidate).unwrap();
        assert_eq!(
            authenticate_target(&source, &input, &target),
            Err(ProviderError::Authentication),
            "mutation {index}"
        );
        candidate.workflow_run.as_mut().unwrap().conclusion = Some(WorkflowRunConclusion::Failure);
        let input = serde_json::to_vec(&candidate).unwrap();
        assert_eq!(authenticate_target(&source, &input, &target), Ok(None));
    }
}

#[test]
fn pull_request_identity_excludes_root_metadata() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let original = authenticate_target(&source, &BODY, &target)
        .unwrap()
        .unwrap();
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    let root = payload.repository.as_mut().unwrap();
    root.node_id = "root-metadata-only".to_owned();
    root.owner.name = Some(amiss_wire::assessment::Nullable::Null);
    root.network_count = Some(1_u32.into());
    root.organization = Some(amiss_wire::assessment::Nullable::Value(
        RepositoryOrganization::Name("unrelated-metadata".to_owned()),
    ));
    let input = serde_json::to_vec(&payload).unwrap();
    let accepted = authenticate_target(&source, &input, &target)
        .unwrap()
        .unwrap();
    assert_eq!(accepted.delivery(), original.delivery());
    let mutations: [fn(&mut PullRepositoryRecord); 4] = [
        |base| base.id += 1,
        |base| base.name = "other".to_owned(),
        |base| base.full_name = "other/widget".to_owned(),
        |base| base.owner.login = "other".to_owned(),
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut changed = payload.clone();
        mutate(
            changed
                .pull_request
                .as_mut()
                .unwrap()
                .request
                .base
                .repo
                .as_mut()
                .unwrap(),
        );
        let input = serde_json::to_vec(&changed).unwrap();
        assert_eq!(
            authenticate_target(&source, &input, &target),
            Err(ProviderError::Authentication),
            "mutation {index}"
        );
    }
}

#[test]
fn signed_webhook_metadata_is_complete_closed_and_checked() {
    let completion_source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let body = serde_json::to_string(&workflow_payload()).unwrap();
    assert!(matches!(
        authenticate_target(&completion_source, body.as_bytes(), &target),
        Ok(Some(_))
    ));
    for (old, new) in [
        (
            r#""node_id":"installation-seven""#,
            r#""node_id":"installation-seven","unknown":true"#,
        ),
        (r#","node_id":"installation-seven""#, ""),
        (r#""node_id":"installation-seven""#, r#""node_id":false"#),
        (r#""id":7"#, r#""id":9007199254740992"#),
        (
            r#""node_id":"workflow-321""#,
            r#""node_id":"workflow-321","unknown":true"#,
        ),
        (r#","node_id":"workflow-321""#, ""),
        (r#""name":"Documentation evidence""#, r#""name":null"#),
        (r#""id":321"#, r#""id":9007199254740992"#),
    ] {
        assert_eq!(body.matches(old).count(), 1, "{old}");
        let changed = body.replacen(old, new, 1);
        assert!(
            matches!(
                authenticate_target(&completion_source, changed.as_bytes(), &target),
                Err(ProviderError::Authentication)
            ),
            "accepted {new}"
        );
    }
    let body = std::str::from_utf8(&BODY).unwrap();
    for (old, new) in [
        (
            r#""node_id":"installation-seven""#,
            r#""node_id":"installation-seven","unknown":true"#,
        ),
        (r#","node_id":"installation-seven""#, ""),
        (r#""id":7"#, r#""id":9007199254740992"#),
    ] {
        assert_eq!(body.matches(old).count(), 1, "{old}");
        let changed = body.replacen(old, new, 1);
        assert!(
            matches!(
                authenticate_target(&source(), changed.as_bytes(), &target),
                Err(ProviderError::Authentication)
            ),
            "accepted {new}"
        );
    }
}

#[test]
fn workflow_pr_references_reject_unknown_missing_and_malformed_metadata() {
    let completion_source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let body = serde_json::to_string(&workflow_payload()).unwrap();
    assert!(matches!(
        authenticate_target(&completion_source, body.as_bytes(), &target),
        Ok(Some(_))
    ));
    for (old, new) in [
        (
            r#""url":"https://api.github.com/repos/HardMax71/widget/pulls/42""#,
            r#""url":"https://api.github.com/repos/HardMax71/widget/pulls/42","unknown":true"#,
        ),
        (
            r#","url":"https://api.github.com/repos/HardMax71/widget/pulls/42""#,
            "",
        ),
        (
            r#""url":"https://api.github.com/repos/Contributor/widget""#,
            r#""url":"https://api.github.com/repos/Contributor/widget","unknown":true"#,
        ),
        (
            r#""url":"https://api.github.com/repos/Contributor/widget""#,
            r#""url":false"#,
        ),
        (r#""id":4201"#, r#""id":9007199254740992"#),
    ] {
        assert_eq!(body.matches(old).count(), 1, "{old}");
        let changed = body.replacen(old, new, 1);
        assert!(
            matches!(
                authenticate_target(&completion_source, changed.as_bytes(), &target),
                Err(ProviderError::Authentication)
            ),
            "accepted {new}"
        );
    }
    for from in ['a', 'b'] {
        let original = from.to_string().repeat(40);
        let other_format = from.to_string().repeat(64);
        assert!(body.contains(&original));
        let changed = body.replace(&original, &other_format);
        assert!(matches!(
            authenticate_target(&completion_source, changed.as_bytes(), &target),
            Err(ProviderError::Authentication)
        ));
    }
    let other_format = replaced_once(&BODY, &"b".repeat(40), &"b".repeat(64));
    assert!(matches!(
        authenticate_target(&source(), &other_format, &target),
        Err(ProviderError::Authentication)
    ));
}

#[test]
fn contradictory_configured_completion_fields_fail_authentication() {
    let source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let changes: [fn(&mut Workflow, &mut WorkflowRun); 9] = [
        |workflow, _| workflow.id = 999,
        |_, run| run.status = CheckRunStatus::InProgress,
        |_, run| run.run_attempt = 0,
        |_, run| run.head_sha = oid('f'),
        |_, run| run.repository.full_name = "other/widget".to_owned(),
        |_, run| run.pull_requests[0].as_mut().unwrap().head.repo.id = 999,
        |_, run| run.pull_requests[0].as_mut().unwrap().base.repo.id = 999,
        |_, run| run.pull_requests[0].as_mut().unwrap().head.repo.name = "other".to_owned(),
        |_, run| run.pull_requests[0].as_mut().unwrap().base.repo.name = "other".to_owned(),
    ];
    for (index, change) in changes.into_iter().enumerate() {
        let mut payload = workflow_payload();
        change(
            payload.workflow.as_mut().unwrap(),
            payload.workflow_run.as_mut().unwrap(),
        );
        let body = serde_json::to_vec(&payload).unwrap();
        assert_eq!(
            authenticate_target(&source, &body, &target),
            Err(ProviderError::Authentication),
            "change {index}"
        );
    }
}

#[test]
fn signed_irrelevant_deliveries_are_authenticated_without_work() {
    let source = source();
    let main = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let check_run = amiss_fixtures::GITHUB_WEBHOOK_CHECK_RUN;
    assert_eq!(authenticate_target(&source, check_run, &main), Ok(None));

    let check_suite = amiss_fixtures::GITHUB_WEBHOOK_CHECK_SUITE;
    assert_eq!(authenticate_target(&source, check_suite, &main), Ok(None));

    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    payload.pull_request = None;
    let issue = serde_json::to_string(&payload).unwrap().replacen(
        '{',
        r#"{"issue":{"id":1,"number":5},"#,
        1,
    );
    assert_eq!(
        authenticate_target(&source, issue.as_bytes(), &main),
        Ok(None)
    );

    let closed = replaced_once(&BODY, r#""action":"opened""#, r#""action":"closed""#);
    assert_eq!(authenticate_target(&source, &closed, &main), Ok(None));

    let title_change = replaced_once(
        &BODY,
        r#""action":"opened","#,
        r#""action":"edited","changes":{"title":{"from":"old title"}},"#,
    );
    assert_eq!(authenticate_target(&source, &title_change, &main), Ok(None));
}

#[test]
fn malformed_supported_delivery_is_not_no_work() {
    let source = source();
    let main = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for malformed in [
        br#"{"action":"opened","pull_request":{}}"#.as_slice(),
        br#"{"action":"completed","check_suite":{"id":9321},"installation":{"id":7,"node_id":"installation-seven"}}"#.as_slice(),
    ] {
        assert_eq!(
            authenticate_target(&source, malformed, &main),
            Err(ProviderError::Authentication)
        );
    }
}

#[test]
fn signed_active_pull_requests_reject_unknown_metadata() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let body = std::str::from_utf8(&BODY).unwrap();
    for action in ["opened", "reopened", "edited", "synchronize"] {
        let body = body.replacen(
            r#""action":"opened""#,
            &format!(r#""action":"{action}""#),
            1,
        );
        for member in [r#""pull_request":{"#, r#""head":{"#, r#""base":{"#] {
            assert_eq!(body.matches(member).count(), 1);
            let invalid = body.replacen(member, &format!(r#"{member}"unknown":true,"#), 1);
            assert_eq!(
                authenticate_target(&source, invalid.as_bytes(), &target),
                Err(ProviderError::Authentication),
                "{action}: unknown metadata in {member}",
            );
        }
    }
}

#[test]
fn signed_actions_select_complete_pull_contracts_without_fallback() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let original = authenticate_target(&source, &BODY, &target)
        .unwrap()
        .unwrap();
    let body = std::str::from_utf8(&BODY).unwrap();
    for action in ["opened", "reopened", "synchronize"] {
        let body = body.replacen(
            r#""action":"opened""#,
            &format!(r#""action":"{action}""#),
            1,
        );
        let decoded: GitHubEvent = serde_json::from_str(&body).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(body.as_bytes()).unwrap(),
            amiss_fixtures::canonical_json(&serde_json::to_vec(&decoded).unwrap()).unwrap(),
        );
        let accepted = authenticate_target(&source, body.as_bytes(), &target)
            .unwrap()
            .unwrap();
        assert_eq!(accepted.delivery(), original.delivery());
        for (member, accepted) in [
            (r#""mergeable":null,"#, action == "synchronize"),
            (r#""draft":false,"#, action != "synchronize"),
        ] {
            assert_eq!(body.matches(member).count(), 1);
            let changed = body.replacen(member, "", 1);
            let result = authenticate_target(&source, changed.as_bytes(), &target);
            if accepted {
                assert_eq!(result.unwrap().unwrap().delivery(), original.delivery());
            } else {
                assert_eq!(result, Err(ProviderError::Authentication));
            }
        }
    }
    for action in [
        r#""unknown""#,
        r#"{"opened":null}"#,
        r#""opened","action":"closed""#,
        r#""opened","\u0061ction":"synchronize""#,
    ] {
        let changed = body.replacen(r#""action":"opened""#, &format!(r#""action":{action}"#), 1);
        assert_eq!(
            authenticate_target(&source, changed.as_bytes(), &target),
            Err(ProviderError::Authentication)
        );
    }
}

#[test]
fn signed_nullable_refs_preserve_binding_and_missing_actions_stay_no_work() {
    let source = source();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let original = authenticate_target(&source, &BODY, &target)
        .unwrap()
        .unwrap();
    let mut ordinary: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    ordinary.pull_request.as_mut().unwrap().request.head.repo = None;
    let wire = serde_json::to_vec(&ordinary).unwrap();
    assert_eq!(
        authenticate_target(&source, &wire, &target)
            .unwrap()
            .unwrap()
            .delivery(),
        original.delivery()
    );
    ordinary.action = None;
    let wire = serde_json::to_vec(&ordinary).unwrap();
    assert_eq!(authenticate_target(&source, &wire, &target), Ok(None));
    ordinary.action = Some("opened".to_owned());
    ordinary.pull_request.as_mut().unwrap().request.base.repo = None;
    let wire = serde_json::to_vec(&ordinary).unwrap();
    assert_eq!(
        authenticate_target(&source, &wire, &target),
        Err(ProviderError::Authentication)
    );

    let mut synchronize: GitHubPayload<SynchronizePullRequest> =
        serde_json::from_slice(&BODY).unwrap();
    synchronize.action = Some("synchronize".to_owned());
    let pull = synchronize.pull_request.as_mut().unwrap();
    pull.head.repo = None;
    pull.head.user = None;
    pull.user = amiss_wire::assessment::Nullable::Null;
    pull.mergeable = None;
    let wire = serde_json::to_vec(&synchronize).unwrap();
    assert_eq!(
        authenticate_target(&source, &wire, &target)
            .unwrap()
            .unwrap()
            .delivery(),
        original.delivery()
    );
    synchronize.action = None;
    let wire = serde_json::to_vec(&synchronize).unwrap();
    assert_eq!(authenticate_target(&source, &wire, &target), Ok(None));
    synchronize.action = Some("synchronize".to_owned());
    synchronize.pull_request.as_mut().unwrap().base.repo.owner = None;
    let wire = serde_json::to_vec(&synchronize).unwrap();
    assert_eq!(
        authenticate_target(&source, &wire, &target),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn edited_requires_a_signed_base_change() {
    let adapter = adapter(FakeApi::new(dummy_snapshot()));
    let mut payload: GitHubPayload = serde_json::from_slice(&BODY).unwrap();
    payload.action = Some("edited".to_owned());
    payload.changes = Some(PullRequestChanges {
        base: Some(BaseChange {
            reference: PreviousReference {
                from: "main".to_owned(),
            },
            sha: PreviousReference {
                from: Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap(),
            },
        }),
        body: None,
        title: None,
    });
    let base_change = serde_json::to_vec(&payload).unwrap();
    let accepted = authenticated(
        &adapter,
        &base_change,
        &[],
        SignedTimePolicy::ReplayOnly,
        provider(),
    )
    .unwrap();
    assert_eq!(
        accepted.delivery().provider_run.candidate_commit.as_str(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );

    payload.changes = Some(PullRequestChanges {
        base: None,
        body: None,
        title: Some(PreviousReference {
            from: "old title".to_owned(),
        }),
    });
    let title_change = serde_json::to_vec(&payload).unwrap();
    assert_eq!(
        authenticated(
            &adapter,
            &title_change,
            &[],
            SignedTimePolicy::ReplayOnly,
            provider(),
        ),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn signed_edited_changes_refuse_partial_or_unknown_records() {
    let source = source();
    let main = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let changes: PullRequestChanges =
        serde_json::from_str(include_str!("../fixtures/webhook-pull-changes.json")).unwrap();
    let wire = serde_json::to_string(&changes).unwrap();
    let body = replaced_once(
        &BODY,
        r#""action":"opened","#,
        &format!(r#""action":"edited","changes":{wire},"#),
    );
    assert!(
        authenticate_target(&source, &body, &main)
            .unwrap()
            .is_some()
    );
    for (old, new) in [
        (r#""base":{"#, r#""unknown":true,"base":{"#),
        (r#""base":{"#, r#""base":{"unknown":true,"#),
        (r#""ref":{"#, r#""ref":{"unknown":true,"#),
        (r#""sha":{"#, r#""sha":{"unknown":true,"#),
        (r#""body":{"#, r#""body":{"unknown":true,"#),
        (r#""title":{"#, r#""title":{"unknown":true,"#),
        (
            r#""title":{"#,
            r#""\u0074itle":{"from":"duplicate"},"title":{"#,
        ),
        (
            r#""from":"previous-main""#,
            r#""\u0066rom":"duplicate","from":"previous-main""#,
        ),
        (r#""ref":{"from":"previous-main"},"#, ""),
        (
            r#","sha":{"from":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
            "",
        ),
        (r#""from":"previous-main""#, r#""from":null"#),
        ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "invalid-oid"),
        (r#""from":"Previous documentation details.""#, ""),
        (r#""from":"Previous pull request title""#, r#""from":null"#),
    ] {
        assert_eq!(wire.matches(old).count(), 1, "{old}");
        let invalid = wire.replacen(old, new, 1);
        let body = replaced_once(
            &BODY,
            r#""action":"opened","#,
            &format!(r#""action":"edited","changes":{invalid},"#),
        );
        assert_eq!(
            authenticate_target(&source, &body, &main),
            Err(ProviderError::Authentication),
            "{invalid}"
        );
    }
    for input in [
        "{}",
        r#"{"body":{"from":""}}"#,
        r#"{"title":{"from":"Old title"}}"#,
        r#"{"body":{"from":"Old body"},"title":{"from":"Old title"}}"#,
    ] {
        let body = replaced_once(
            &BODY,
            r#""action":"opened","#,
            &format!(r#""action":"edited","changes":{input},"#),
        );
        assert_eq!(authenticate_target(&source, &body, &main), Ok(None));
    }
}

#[test]
fn signed_source_outlives_the_live_api_adapter() {
    let source = Arc::new(source());
    let dropped = Arc::new(AtomicBool::new(false));
    let adapter = GitHubPullRequestAdapter::from_source(
        Arc::clone(&source),
        DropApi {
            dropped: Arc::clone(&dropped),
        },
    );
    let through_adapter = authenticated(
        &adapter,
        &BODY,
        &[],
        SignedTimePolicy::ReplayOnly,
        provider(),
    )
    .unwrap();
    drop(adapter);
    assert!(dropped.load(Ordering::Acquire));
    let through_source = authenticated(
        source.as_ref(),
        &BODY,
        &[],
        SignedTimePolicy::ReplayOnly,
        provider(),
    )
    .unwrap();
    assert_eq!(through_adapter, through_source);
}

#[test]
fn rejects_malformed_or_internally_inconsistent_signed_payloads() {
    let cases = [
        replaced(&BODY, r#""id":7"#, r#""id":0"#),
        replaced(&BODY, r#""id":101"#, r#""id":0"#),
        replaced(&BODY, r#""id":4201"#, r#""id":0"#),
        replaced(&BODY, r#""number":42"#, r#""number":0"#),
        replaced_once(&BODY, r#""id":101"#, r#""id":102"#),
        replaced_once(&BODY, r#""number":42"#, r#""number":41"#),
        replaced_once(&BODY, r#""action":"opened""#, r#""action":"edited""#),
        replaced_once(&BODY, "HardMax71/widget", "HardMax71/other"),
        replaced_once(&BODY, r#""name":"widget""#, r#""name":"other""#),
        replaced(
            &BODY,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
        ),
        replaced_once(&BODY, r#""ref":"topic""#, r#""ref":"bad ref""#),
        replaced_once(&BODY, r#""ref":"main""#, r#""ref":"bad..ref""#),
        br#"{"installation":{"id":7,"node_id":"installation-seven"}}"#.to_vec(),
    ];
    for body in cases {
        let adapter = adapter(FakeApi::new(dummy_snapshot()));
        let result = authenticated(
            &adapter,
            &body,
            &[],
            SignedTimePolicy::ReplayOnly,
            provider(),
        );
        assert_eq!(result, Err(ProviderError::Authentication));
    }
}

#[test]
fn rejects_body_tampering_and_wrong_routes() {
    let adapter = adapter(FakeApi::new(dummy_snapshot()));
    let signed = signature(&BODY);
    let tampered = replaced_once(&BODY, r#""number":42"#, r#""number":43"#);
    assert_eq!(
        try_authenticate_with_signature(
            &adapter,
            &tampered,
            &signed,
            &[],
            SignedTimePolicy::ReplayOnly,
            provider(),
        ),
        Err(ProviderError::Authentication)
    );

    let wrong_provider = ProviderIdentity {
        namespace: ProviderNamespace::try_from("github".to_owned()).unwrap(),
        instance: ProviderInstance::try_from("github.enterprise.test".to_owned()).unwrap(),
    };
    assert_eq!(
        authenticated(
            &adapter,
            &BODY,
            &[],
            SignedTimePolicy::ReplayOnly,
            wrong_provider,
        ),
        Err(ProviderError::Authentication)
    );
    assert_eq!(
        authenticated(
            &adapter,
            &BODY,
            &[],
            SignedTimePolicy::Required(Duration::from_mins(5)),
            provider(),
        ),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn refresh_marks_ref_drift_superseded() {
    let seed = adapter(FakeApi::new(dummy_snapshot()));
    let verified =
        authenticated(&seed, &BODY, &[], SignedTimePolicy::ReplayOnly, provider()).unwrap();
    let delivery = verified.delivery().clone();
    let exact = snapshot(&delivery, "topic", "main");
    let exact_api = FakeApi::new(exact.clone());
    let exact_adapter = adapter(exact_api.clone());
    assert_eq!(exact_adapter.refresh(&delivery), Ok(exact));
    assert_eq!(exact_api.state.refreshes.load(Ordering::Relaxed), 1);
    assert_eq!(
        *exact_api.state.requests.lock().unwrap(),
        [ApiRequest {
            installation_id: 7,
            repository_id: 101,
            owner: "hardmax71".to_owned(),
            name: "widget".to_owned(),
            pull_request_id: 4201,
            number: 42,
            candidate: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        }]
    );

    let drifted = snapshot(&delivery, "other", "main");
    let drifted_api = FakeApi::new(drifted.clone());
    let drifted_adapter = adapter(drifted_api.clone());
    assert_eq!(
        drifted_adapter.refresh(&delivery),
        Ok(ChangeSnapshot {
            state: ChangeState::Superseded,
            run: drifted.run,
            gate_commit: drifted.gate_commit,
        })
    );
    assert_eq!(drifted_api.state.refreshes.load(Ordering::Relaxed), 1);

    let mut wrong_candidate = snapshot(&delivery, "topic", "main");
    wrong_candidate.run.commits.candidate = oid('e');
    let wrong_api = FakeApi::new(wrong_candidate);
    let wrong_adapter = adapter(wrong_api.clone());
    assert_eq!(
        wrong_adapter.refresh(&delivery),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(wrong_api.state.refreshes.load(Ordering::Relaxed), 1);

    let mut invalid_delivery = delivery.clone();
    invalid_delivery.provider_run.run_id = OpaqueId::try_from("unbound".to_owned()).unwrap();
    let refused_api = FakeApi::new(snapshot(&delivery, "topic", "main"));
    let refused_adapter = adapter(refused_api.clone());
    assert_eq!(
        refused_adapter.refresh(&invalid_delivery),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(refused_api.state.refreshes.load(Ordering::Relaxed), 0);
}

#[test]
fn publication_is_delegated_only_under_the_authenticated_identity() {
    let seed = adapter(FakeApi::new(dummy_snapshot()));
    let verified =
        authenticated(&seed, &BODY, &[], SignedTimePolicy::ReplayOnly, provider()).unwrap();
    let delivery = verified.delivery().clone();
    let run = snapshot(&delivery, "topic", "main").run;
    let valid = publication(&delivery, run.clone());
    let api = FakeApi::new(ChangeSnapshot {
        state: ChangeState::Active,
        run: run.clone(),
        gate_commit: oid('e'),
    });
    let adapter = adapter(api.clone());
    assert_eq!(adapter.publish(&delivery, &valid), Ok(()));
    assert_eq!(api.state.publications.lock().unwrap().len(), 1);

    let invalid = publication(&delivery, snapshot(&delivery, "changed", "main").run);
    assert_eq!(
        adapter.publish(&delivery, &invalid),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(api.state.publications.lock().unwrap().len(), 1);

    let mut cancelled = publication(&delivery, snapshot(&delivery, "changed", "main").run);
    cancelled.conclusion = CheckConclusion::Superseded;
    cancelled.report = None;
    assert_eq!(adapter.publish(&delivery, &cancelled), Ok(()));
    assert_eq!(api.state.publications.lock().unwrap().len(), 2);
}

/// The delivery is bound to the source on eight separate clauses, so each is
/// broken alone: a run that fails two of them proves neither.
#[test]
fn every_clause_binding_the_delivery_stands_alone() {
    let seed = adapter(FakeApi::new(dummy_snapshot()));
    let verified =
        authenticated(&seed, &BODY, &[], SignedTimePolicy::ReplayOnly, provider()).unwrap();
    let delivery = verified.delivery().clone();
    let elsewhere = ProviderIdentity {
        namespace: ProviderNamespace::try_from("github".to_owned()).unwrap(),
        instance: ProviderInstance::try_from("github.example".to_owned()).unwrap(),
    };

    let mut foreign_identity = delivery.clone();
    foreign_identity.identity.provider = elsewhere.clone();
    let mut foreign_change = delivery.clone();
    foreign_change.change.provider = elsewhere;
    let mut other_host = delivery.clone();
    other_host.change.repository = RepositoryIdentity::new(
        "github.example".to_owned(),
        "hardmax71".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    assert!(
        RepositoryIdentity::github("HardMax71".to_owned(), "widget".to_owned()).is_none(),
        "a non-canonical owner cannot enter an authenticated delivery"
    );
    let mut retried = delivery.clone();
    retried.provider_run.attempt = ProviderRunAttempt::try_from(2).unwrap();
    let mut wider_format = delivery.clone();
    wider_format.provider_run.object_format = ObjectFormat::Sha256;
    let mut wider_candidate = delivery;
    wider_candidate.provider_run.candidate_commit =
        Oid::new(ObjectFormat::Sha256, "b".repeat(64)).unwrap();

    for broken in [
        foreign_identity,
        foreign_change,
        other_host,
        retried,
        wider_format,
        wider_candidate,
    ] {
        let api = FakeApi::new(dummy_snapshot());
        assert_eq!(
            adapter(api.clone()).refresh(&broken),
            Err(ProviderError::InvalidResponse),
            "{broken:?}"
        );
        assert_eq!(
            api.state.refreshes.load(Ordering::Relaxed),
            0,
            "a delivery that fails its binding never reaches the provider"
        );
    }
}

fn adapter(api: FakeApi) -> GitHubPullRequestAdapter<FakeApi> {
    GitHubPullRequestAdapter::new(provider(), webhook(), api)
}

fn source() -> GitHubPullRequestSource {
    GitHubPullRequestSource::new(provider(), webhook(), &[])
}

fn workflow_artifact(workflow_identity: &str) -> WorkflowArtifactExpectation {
    WorkflowArtifactExpectation {
        provider: provider(),
        repository: RepositoryIdentity::new(
            "github.com".to_owned(),
            "hardmax71".to_owned(),
            "widget".to_owned(),
        )
        .unwrap(),
        workflow_identity: OpaqueId::try_from(workflow_identity.to_owned()).unwrap(),
        event: OpaqueId::try_from("pull_request".to_owned()).unwrap(),
        artifact_name: "amiss-semantic-evidence".to_owned(),
        payload_file: RepoPathText::new("amiss/semantic-template.json".to_owned()).unwrap(),
        archive_byte_limit: 1_048_576,
        file_byte_limit: 524_288,
        semantic: SemanticEvidenceExpectation {
            acquisition_identity: ArtifactId::new("github-docs-evidence".to_owned()).unwrap(),
            producer_kind: amiss_wire::semantic::SemanticProducerKind::SiteBuild,
            producer_identity: ArtifactId::new("docs-site".to_owned()).unwrap(),
            producer_version: "0.5.1".to_owned(),
            context_digest: hb("amiss/test-workflow-completion", b"context"),
        },
    }
}

fn workflow_payload() -> GitHubPayload {
    let mut run: WorkflowRun =
        serde_json::from_slice(include_bytes!("../fixtures/webhook-workflow-run.json")).unwrap();
    run.id = 9_001;
    "pull_request".clone_into(&mut run.event);
    run.workflow_id = 321;
    run.run_attempt = 2;
    run.head_sha = oid('b');
    run.head_commit.id = oid('b');
    run.head_branch = amiss_wire::assessment::Nullable::Value("topic".to_owned());
    ".github/workflows/docs-evidence.yml".clone_into(&mut run.path);
    let run_repository = &mut run.repository;
    run_repository.id = 101;
    "widget".clone_into(&mut run_repository.name);
    "HardMax71/widget".clone_into(&mut run_repository.full_name);
    "HardMax71".clone_into(&mut run_repository.owner.as_mut().unwrap().login);
    let mut head_repository = run_repository.clone();
    head_repository.id = 202;
    "Contributor/widget".clone_into(&mut head_repository.full_name);
    "Contributor".clone_into(&mut head_repository.owner.as_mut().unwrap().login);
    run.head_repository = head_repository;
    run.pull_requests = vec![Some(WorkflowPullRequest {
        id: 4_201,
        number: 42,
        url: "https://api.github.com/repos/HardMax71/widget/pulls/42".to_owned(),
        head: WorkflowPullRef {
            branch: "topic".to_owned(),
            sha: oid('b'),
            repo: WorkflowPullRepository {
                id: 202,
                name: "widget".to_owned(),
                url: "https://api.github.com/repos/Contributor/widget".to_owned(),
            },
        },
        base: WorkflowPullRef {
            branch: "main".to_owned(),
            sha: oid('a'),
            repo: WorkflowPullRepository {
                id: 101,
                name: "widget".to_owned(),
                url: "https://api.github.com/repos/HardMax71/widget".to_owned(),
            },
        },
    })];
    let mut repository: PullRepositoryRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY).unwrap();
    repository.id = 101;
    "widget".clone_into(&mut repository.name);
    "HardMax71/widget".clone_into(&mut repository.full_name);
    "HardMax71".clone_into(&mut repository.owner.login);
    GitHubPayload {
        action: Some("completed".to_owned()),
        changes: None,
        installation: Some(Installation {
            id: 7,
            node_id: "installation-seven".to_owned(),
        }),
        repository: Some(repository),
        number: None,
        pull_request: None,
        review: None,
        comment: None,
        thread: None,
        check_suite: None,
        check_run: None,
        workflow: Some(Workflow {
            id: 321,
            node_id: "workflow-321".to_owned(),
            name: "Documentation evidence".to_owned(),
            path: ".github/workflows/docs-evidence.yml".to_owned(),
            state: "active".to_owned(),
            created_at: "2020-10-02T12:42:30.000Z".to_owned(),
            updated_at: "2020-10-03T19:24:48.000Z".to_owned(),
            url: "https://api.github.com/repos/HardMax71/widget/actions/workflows/321".to_owned(),
            html_url:
                "https://github.com/HardMax71/widget/blob/main/.github/workflows/docs-evidence.yml"
                    .to_owned(),
            badge_url:
                "https://github.com/HardMax71/widget/workflows/Documentation%20evidence/badge.svg"
                    .to_owned(),
        }),
        workflow_run: Some(run),
    }
}

fn webhook() -> GitHubWebhook {
    let trust_set = OpaqueId::try_from("github-webhooks".to_owned()).unwrap();
    let key = WebhookKey::new(
        OpaqueId::try_from("current".to_owned()).unwrap(),
        SECRET.to_vec(),
        0,
        None,
    )
    .unwrap();
    GitHubWebhook::new(WebhookKeyring::new(trust_set, vec![key]).unwrap())
}

fn observed(pull_request: GitHubPullRequest<'_>) -> ApiRequest {
    ApiRequest {
        installation_id: pull_request.installation_id,
        repository_id: pull_request.repository_id,
        owner: pull_request.repository_owner.to_owned(),
        name: pull_request.repository_name.to_owned(),
        pull_request_id: pull_request.pull_request_id,
        number: pull_request.number,
        candidate: pull_request.candidate_commit.as_str().to_owned(),
    }
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::try_from("github".to_owned()).unwrap(),
        instance: ProviderInstance::try_from("github.com".to_owned()).unwrap(),
    }
}

fn authenticated(
    source: &dyn SignedSource,
    body: &[u8],
    unsigned: &[(&str, &[u8])],
    signed_time: SignedTimePolicy,
    route_provider: ProviderIdentity,
) -> Result<amiss_controller::VerifiedDelivery, ProviderError> {
    try_authenticate_with_signature(
        source,
        body,
        &signature(body),
        unsigned,
        signed_time,
        route_provider,
    )
}

fn try_authenticate_with_signature(
    source: &dyn SignedSource,
    body: &[u8],
    signature: &[u8],
    unsigned: &[(&str, &[u8])],
    signed_time: SignedTimePolicy,
    route_provider: ProviderIdentity,
) -> Result<amiss_controller::VerifiedDelivery, ProviderError> {
    let route = DeliveryRoute {
        provider: route_provider,
        trust_set: OpaqueId::try_from("github-webhooks".to_owned()).unwrap(),
        signed_time,
    };
    let mut headers = Vec::with_capacity(unsigned.len().saturating_add(1));
    headers.push(DeliveryHeader {
        name: "x-hub-signature-256",
        value: signature,
    });
    headers.extend(
        unsigned
            .iter()
            .map(|(name, value)| DeliveryHeader { name, value }),
    );
    let policy = policy();
    let check = policy
        .pre_auth(
            UntrustedDelivery {
                route: &route,
                received_at_unix_millis: NOW,
                headers: &headers,
                body,
            },
            &*TestClock::at(NOW),
        )
        .unwrap();
    source.authenticate_delivery(check)
}

fn policy() -> IngressPolicy {
    IngressPolicy::new(
        IngressLimits::new(1_000_000, 16, 4_096).unwrap(),
        ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1)).unwrap(),
        Duration::ZERO,
    )
    .unwrap()
}

fn authenticate_target(
    source: &GitHubPullRequestSource,
    body: &[u8],
    target: &BranchRef,
) -> Result<Option<amiss_controller::VerifiedDelivery>, ProviderError> {
    let signature = signature(body);
    let headers = [DeliveryHeader {
        name: "x-hub-signature-256",
        value: &signature,
    }];
    let route = DeliveryRoute {
        provider: provider(),
        trust_set: OpaqueId::try_from("github-webhooks".to_owned()).unwrap(),
        signed_time: SignedTimePolicy::ReplayOnly,
    };
    let check = policy()
        .pre_auth(
            UntrustedDelivery {
                route: &route,
                received_at_unix_millis: NOW,
                headers: &headers,
                body,
            },
            &*TestClock::at(NOW),
        )
        .unwrap();
    source.authenticate_for_target(check, target)
}

fn signature(body: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(SECRET).unwrap();
    mac.update(body);
    let mut encoded = b"sha256=".to_vec();
    encoded.extend(hex::encode(mac.finalize().into_bytes()).bytes());
    encoded
}

fn snapshot(
    delivery: &AuthenticatedDelivery,
    candidate_ref: &str,
    target_ref: &str,
) -> ChangeSnapshot {
    let candidate = delivery.provider_run.candidate_commit.clone();
    ChangeSnapshot {
        state: ChangeState::Active,
        run: RunIdentity::new(
            delivery.change.clone(),
            RunRefs {
                forge: ForgeDialect::Github,
                candidate: BranchRef::new(format!("refs/heads/{candidate_ref}")).unwrap(),
                target: BranchRef::new(format!("refs/heads/{target_ref}")).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: oid('a'),
                candidate,
            },
            OidPair {
                base: oid('c'),
                candidate: oid('d'),
            },
        )
        .unwrap(),
        gate_commit: oid('e'),
    }
}

fn dummy_snapshot() -> ChangeSnapshot {
    let provider = provider();
    let repository = RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap();
    let change = amiss_controller::ChangeLocator {
        provider,
        repository,
        change: OpaqueId::try_from("42".to_owned()).unwrap(),
    };
    ChangeSnapshot {
        state: ChangeState::Active,
        run: RunIdentity::new(
            change,
            RunRefs {
                forge: ForgeDialect::Github,
                candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
                target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: oid('a'),
                candidate: oid('b'),
            },
            OidPair {
                base: oid('c'),
                candidate: oid('d'),
            },
        )
        .unwrap(),
        gate_commit: oid('e'),
    }
}

fn publication(delivery: &AuthenticatedDelivery, run: RunIdentity) -> Publication {
    let digest = hb("amiss/controller-github-test", b"fixture");
    Publication {
        provider_run: delivery.provider_run.clone(),
        evaluation_id: ControllerEvaluationId::try_from("evaluation-1".to_owned()).unwrap(),
        check: CheckBinding {
            plan_digest: digest,
            required_status_name: "amiss".to_owned(),
            execution_constraint_digest: digest,
        },
        run,
        gate_commit: oid('e'),
        conclusion: CheckConclusion::Pass,
        report: Some(
            amiss_fixtures::captured_report(amiss_fixtures::SCANNER_REPORT.to_vec()).unwrap(),
        ),
        artifact: None,
    }
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}

fn replaced(source: &[u8], from: &str, to: &str) -> Vec<u8> {
    String::from_utf8(source.to_vec())
        .unwrap()
        .replace(from, to)
        .into_bytes()
}

fn replaced_once(source: &[u8], from: &str, to: &str) -> Vec<u8> {
    String::from_utf8(source.to_vec())
        .unwrap()
        .replacen(from, to, 1)
        .into_bytes()
}
