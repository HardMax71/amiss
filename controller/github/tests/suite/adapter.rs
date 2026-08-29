#![expect(
    clippy::unwrap_used,
    reason = "fixed provider payloads and protocol identities must fail loudly"
)]

use amiss_controller_fixtures::clock::TestClock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, CheckBinding, CheckConclusion,
    ControllerEvaluationId, DeliveryHeader, DeliveryRoute, GitHubWebhook, IngressCheck,
    IngressLimits, IngressPolicy, OidPair, OpaqueId, ProviderAdapter, ProviderError,
    ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt, Publication,
    ReplayWindow, RunIdentity, RunRefs, SemanticEvidenceExpectation, SignedTimePolicy,
    UntrustedDelivery, WebhookKey, WebhookKeyring, WorkflowArtifactExpectation,
};
use amiss_controller_github::{
    GitHubApi, GitHubPullRequest, GitHubPullRequestAdapter, GitHubPullRequestSource,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{
    ArtifactId, BranchRef, ForgeDialect, ObjectFormat, Oid, RepoPathText, RepositoryIdentity,
};
use hmac::{Hmac, KeyInit as _, Mac as _};
use serde_json::json;
use sha2::Sha256;

const NOW: i64 = 1_800_000_000_000;
const SECRET: &[u8] = b"github-webhook-secret";
const BODY: &[u8] = br#"{
  "action":"opened",
  "installation":{"id":7},
  "repository":{
    "id":101,
    "name":"widget",
    "full_name":"HardMax71/widget",
    "owner":{"login":"HardMax71"}
  },
  "number":42,
  "pull_request":{
    "id":4201,
    "number":42,
    "head":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"topic"},
    "base":{
      "ref":"main",
      "repo":{
        "id":101,
        "name":"widget",
        "full_name":"HardMax71/widget",
        "owner":{"login":"HardMax71"}
      }
    }
  }
}"#;

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
        BODY,
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
        BODY,
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
            BODY,
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
        authenticate_target(&source, BODY, &main),
        Ok(Some(_))
    ));
    assert_eq!(
        authenticate_target(&source, BODY, &release),
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
    let pull_request = authenticate_target(&source(), BODY, &target)
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
        authenticate_target(&completion_source, BODY, &target),
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
    for (pointer, value) in [
        ("/action", json!("in_progress")),
        ("/workflow/path", json!(".github/workflows/other.yml")),
        ("/workflow_run/conclusion", json!("failure")),
        ("/workflow_run/pull_requests", json!([])),
    ] {
        let mut payload = workflow_payload();
        *payload.pointer_mut(pointer).unwrap() = value;
        let body = serde_json::to_vec(&payload).unwrap();
        assert_eq!(
            authenticate_target(&source, &body, &target),
            Ok(None),
            "{pointer}"
        );
    }
    let check_run =
        br#"{"action":"completed","check_run":{"id":89721586894},"installation":{"id":7}}"#;
    assert_eq!(authenticate_target(&source, check_run, &target), Ok(None));

    let other_target = BranchRef::new("refs/heads/release".to_owned()).unwrap();
    let body = serde_json::to_vec(&workflow_payload()).unwrap();
    assert_eq!(
        authenticate_target(&source, &body, &other_target),
        Err(ProviderError::AuthorizationRevoked)
    );
}

#[test]
fn contradictory_configured_completion_fields_fail_authentication() {
    let source = GitHubPullRequestSource::new(
        provider(),
        webhook(),
        &[workflow_artifact("docs-evidence.yml")],
    );
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for (pointer, value) in [
        ("/workflow/id", json!(999)),
        ("/workflow_run/status", json!("in_progress")),
        ("/workflow_run/run_attempt", json!(0)),
        ("/workflow_run/head_sha", json!("f".repeat(40))),
        ("/workflow_run/repository/full_name", json!("other/widget")),
        ("/workflow_run/pull_requests/0/head/repo/id", json!(999)),
    ] {
        let mut payload = workflow_payload();
        *payload.pointer_mut(pointer).unwrap() = value;
        let body = serde_json::to_vec(&payload).unwrap();
        assert_eq!(
            authenticate_target(&source, &body, &target),
            Err(ProviderError::Authentication),
            "{pointer}"
        );
    }
}

#[test]
fn signed_irrelevant_deliveries_are_authenticated_without_work() {
    let source = source();
    let main = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    for action in ["created", "completed"] {
        let body = format!(
            r#"{{"action":"{action}","check_run":{{"id":89721586894}},"installation":{{"id":7}}}}"#
        );
        assert_eq!(
            authenticate_target(&source, body.as_bytes(), &main),
            Ok(None)
        );
    }

    let check_suite =
        br#"{"action":"completed","check_suite":{"id":9321},"installation":{"id":7}}"#;
    assert_eq!(authenticate_target(&source, check_suite, &main), Ok(None));

    let issue = br#"{
      "action":"opened",
      "issue":{"id":1,"number":5},
      "repository":{
        "id":101,
        "name":"widget",
        "full_name":"HardMax71/widget",
        "owner":{"login":"HardMax71"}
      },
      "installation":{"id":7}
    }"#;
    assert_eq!(authenticate_target(&source, issue, &main), Ok(None));

    let closed = replaced_once(BODY, r#""action":"opened""#, r#""action":"closed""#);
    assert_eq!(authenticate_target(&source, &closed, &main), Ok(None));

    let title_change = replaced_once(
        BODY,
        r#""action":"opened","#,
        r#""action":"edited","changes":{"title":{"from":"old title"}},"#,
    );
    assert_eq!(authenticate_target(&source, &title_change, &main), Ok(None));
}

#[test]
fn malformed_supported_delivery_is_not_no_work() {
    let source = source();
    let main = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let malformed = br#"{"action":"opened","pull_request":{}}"#;

    assert_eq!(
        authenticate_target(&source, malformed, &main),
        Err(ProviderError::Authentication)
    );
}

#[test]
fn edited_requires_a_signed_base_change() {
    let adapter = adapter(FakeApi::new(dummy_snapshot()));
    let base_change = replaced_once(
        BODY,
        r#""action":"opened","#,
        r#""action":"edited","changes":{"base":{"ref":{"from":"main"}}},"#,
    );
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

    let title_change = replaced_once(
        BODY,
        r#""action":"opened","#,
        r#""action":"edited","changes":{"title":{"from":"old title"}},"#,
    );
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
        BODY,
        &[],
        SignedTimePolicy::ReplayOnly,
        provider(),
    )
    .unwrap();
    drop(adapter);
    assert!(dropped.load(Ordering::Acquire));
    let through_source = authenticated(
        source.as_ref(),
        BODY,
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
        replaced(BODY, r#""id":7"#, r#""id":0"#),
        replaced(BODY, r#""id":101"#, r#""id":0"#),
        replaced(BODY, r#""id":4201"#, r#""id":0"#),
        replaced(BODY, r#""number":42"#, r#""number":0"#),
        replaced_once(BODY, r#""id":101"#, r#""id":102"#),
        replaced_once(BODY, r#""number":42"#, r#""number":41"#),
        replaced_once(BODY, r#""action":"opened""#, r#""action":"edited""#),
        replaced_once(BODY, "HardMax71/widget", "HardMax71/other"),
        replaced_once(BODY, r#""name":"widget""#, r#""name":"other""#),
        replaced(
            BODY,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
        ),
        replaced_once(BODY, r#""ref":"topic""#, r#""ref":"bad ref""#),
        replaced_once(BODY, r#""ref":"main""#, r#""ref":"bad..ref""#),
        br#"{"installation":{"id":7}}"#.to_vec(),
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
    let signed = signature(BODY);
    let tampered = replaced_once(BODY, r#""number":42"#, r#""number":43"#);
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
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.enterprise.test".to_owned()).unwrap(),
    };
    assert_eq!(
        authenticated(
            &adapter,
            BODY,
            &[],
            SignedTimePolicy::ReplayOnly,
            wrong_provider,
        ),
        Err(ProviderError::Authentication)
    );
    assert_eq!(
        authenticated(
            &adapter,
            BODY,
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
        authenticated(&seed, BODY, &[], SignedTimePolicy::ReplayOnly, provider()).unwrap();
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
    invalid_delivery.provider_run.run_id = OpaqueId::new("unbound".to_owned()).unwrap();
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
        authenticated(&seed, BODY, &[], SignedTimePolicy::ReplayOnly, provider()).unwrap();
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
        authenticated(&seed, BODY, &[], SignedTimePolicy::ReplayOnly, provider()).unwrap();
    let delivery = verified.delivery().clone();
    let elsewhere = ProviderIdentity {
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.example".to_owned()).unwrap(),
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
    retried.provider_run.attempt = ProviderRunAttempt::new(2).unwrap();
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
        workflow_identity: OpaqueId::new(workflow_identity.to_owned()).unwrap(),
        event: OpaqueId::new("pull_request".to_owned()).unwrap(),
        artifact_name: "amiss-semantic-evidence".to_owned(),
        payload_file: RepoPathText::new("amiss/semantic-template.json".to_owned()).unwrap(),
        archive_byte_limit: 1_048_576,
        file_byte_limit: 524_288,
        semantic: SemanticEvidenceExpectation {
            acquisition_identity: ArtifactId::new("github-docs-evidence".to_owned()).unwrap(),
            producer_kind: ArtifactId::new("site-build".to_owned()).unwrap(),
            producer_identity: ArtifactId::new("docs-site".to_owned()).unwrap(),
            producer_version: "0.5.1".to_owned(),
            context_digest: hb("amiss/test-workflow-completion", b"context"),
        },
    }
}

fn workflow_payload() -> serde_json::Value {
    let repository = json!({
        "id": 101,
        "name": "widget",
        "full_name": "HardMax71/widget",
        "owner": {"login": "HardMax71"}
    });
    json!({
        "action": "completed",
        "installation": {"id": 7},
        "repository": repository,
        "workflow": {
            "id": 321,
            "path": ".github/workflows/docs-evidence.yml"
        },
        "workflow_run": {
            "id": 9001,
            "event": "pull_request",
            "status": "completed",
            "conclusion": "success",
            "workflow_id": 321,
            "run_attempt": 2,
            "head_sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "repository": repository,
            "head_repository": {
                "id": 202,
                "name": "widget",
                "full_name": "Contributor/widget",
                "owner": {"login": "Contributor"}
            },
            "pull_requests": [{
                "id": 4201,
                "number": 42,
                "head": {
                    "ref": "topic",
                    "sha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "repo": {
                        "id": 202,
                        "name": "widget",
                        "url": "https://api.github.com/repos/Contributor/widget"
                    }
                },
                "base": {
                    "ref": "main",
                    "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "repo": {
                        "id": 101,
                        "name": "widget",
                        "url": "https://api.github.com/repos/HardMax71/widget"
                    }
                }
            }]
        }
    })
}

fn webhook() -> GitHubWebhook {
    let trust_set = OpaqueId::new("github-webhooks".to_owned()).unwrap();
    let key = WebhookKey::new(
        OpaqueId::new("current".to_owned()).unwrap(),
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
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.com".to_owned()).unwrap(),
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
        trust_set: OpaqueId::new("github-webhooks".to_owned()).unwrap(),
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
        trust_set: OpaqueId::new("github-webhooks".to_owned()).unwrap(),
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
        change: OpaqueId::new("42".to_owned()).unwrap(),
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
        evaluation_id: ControllerEvaluationId::new("evaluation-1".to_owned()).unwrap(),
        check: CheckBinding {
            plan_digest: digest,
            required_status_name: "amiss".to_owned(),
            execution_constraint_digest: digest,
        },
        run,
        gate_commit: oid('e'),
        conclusion: CheckConclusion::Pass,
        report: Some(br#"{"schema":"amiss/report"}"#.to_vec()),
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
