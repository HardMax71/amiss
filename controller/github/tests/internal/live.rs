#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed provider fixtures must fail loudly"
)]

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use amiss_controller::{
    ChangeId, ChangeLocator, ChangeState, CheckBinding, CheckConclusion, ControllerEvaluationId,
    IntegrationId, OpaqueId, ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace,
    Publication, RunFailure,
};
use amiss_wire::digest::{hb, sha256};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};

use crate::GitHubPullRequest;

use super::model::{
    BranchRule, CheckRunApp, CheckRunOutputRecord, CheckRunRecord, CommitRecord, CreateCheckRun,
    OwnerRecord, PullRefRecord, PullRepositoryRecord, PullRequestRecord, RefreshData,
    RepositoryRecord,
};
use super::publication::{PublicationDecision, publication_decision, validate_created};
use super::rest::{GitHubRest, OperationDeadline};
use super::{Client, Config};

const APP_ID: u64 = 99;
const INSTALLATION_ID: u64 = 7;

#[test]
fn refresh_maps_exact_trees_and_moved_head() {
    let fixture = Fixture::new();
    let active =
        super::refresh::snapshot(&fixture.config, fixture.request(), &fixture.data).unwrap();
    assert_eq!(active.state, ChangeState::Active);
    assert_eq!(active.run.commits.base, oid('a'));
    assert_eq!(active.run.commits.candidate, oid('b'));
    assert_eq!(active.run.trees.base, oid('c'));
    assert_eq!(active.run.trees.candidate, oid('d'));
    assert_eq!(active.gate_commit, oid('e'));

    let moved = fixture.moved_data();
    let superseded = super::refresh::snapshot(&fixture.config, fixture.request(), &moved).unwrap();
    assert_eq!(superseded.state, ChangeState::Superseded);
    assert_eq!(superseded.run.commits.candidate, oid('b'));
    assert_eq!(superseded.run.trees.candidate, oid('d'));
    assert_eq!(superseded.gate_commit, oid('2'));
}

#[test]
fn refresh_requires_one_exact_ready_merge_gate() {
    let fixture = Fixture::new();

    let mut unknown = fixture.data.clone();
    unknown.pull_request.mergeable = None;
    assert_eq!(
        super::refresh::snapshot(&fixture.config, fixture.request(), &unknown),
        Err(ProviderError::Unavailable)
    );

    let mut conflicted = fixture.data.clone();
    conflicted.pull_request.mergeable = Some(false);
    let snapshot =
        super::refresh::snapshot(&fixture.config, fixture.request(), &conflicted).unwrap();
    assert_eq!(snapshot.state, ChangeState::Superseded);

    let mut wrong_parent = fixture.data.clone();
    *wrong_parent.gate.parents.first_mut().unwrap() = oid('f').as_str().to_owned();
    assert_eq!(
        super::refresh::snapshot(&fixture.config, fixture.request(), &wrong_parent),
        Err(ProviderError::InvalidResponse)
    );

    let mut merged_tree = fixture.data.clone();
    merged_tree.gate.tree = oid('f').as_str().to_owned();
    let snapshot =
        super::refresh::snapshot(&fixture.config, fixture.request(), &merged_tree).unwrap();
    assert_eq!(snapshot.state, ChangeState::Superseded);

    let mut wrong_gate = fixture.data.clone();
    wrong_gate.pull_request.merge_commit_sha = Some(oid('f').as_str().to_owned());
    assert_eq!(
        super::refresh::snapshot(&fixture.config, fixture.request(), &wrong_gate),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn refresh_rejects_wrong_ids_and_github_path_shapes() {
    let fixture = Fixture::new();
    for mutate in [
        |data: &mut RefreshData| data.repository.id = 102,
        |data: &mut RefreshData| data.pull_request.id = 4_202,
        |data: &mut RefreshData| data.pull_request.number = 43,
        |data: &mut RefreshData| {
            data.pull_request.base.repo.as_mut().unwrap().id = 102;
        },
    ] {
        let mut data = fixture.data.clone();
        mutate(&mut data);
        assert_eq!(
            super::refresh::snapshot(&fixture.config, fixture.request(), &data),
            Err(ProviderError::InvalidResponse)
        );
    }

    let mut request = fixture.request();
    request.installation_id = INSTALLATION_ID + 1;
    assert_eq!(
        super::refresh::validate_request(&fixture.config, request),
        Err(ProviderError::InvalidResponse)
    );

    let provider = provider();
    let nested = ChangeLocator {
        provider: provider.clone(),
        repository: RepositoryIdentity::new(
            "github.com".to_owned(),
            "group/owner".to_owned(),
            "widget".to_owned(),
        )
        .unwrap(),
        change: change_id(),
    };
    let nested_request = GitHubPullRequest {
        change: &nested,
        installation_id: INSTALLATION_ID,
        repository_id: 101,
        repository_owner: "group/owner",
        repository_name: "widget",
        pull_request_id: 4_201,
        number: 42,
        candidate_commit: &fixture.candidate,
    };
    assert_eq!(
        super::refresh::validate_request(&fixture.config, nested_request),
        Err(ProviderError::InvalidResponse)
    );

    let mut inconsistent = fixture.change.clone();
    inconsistent.change = ChangeId::new("repository/101/pull/4201/number/43".to_owned()).unwrap();
    let request = GitHubPullRequest {
        change: &inconsistent,
        ..fixture.request()
    };
    assert_eq!(
        super::refresh::validate_request(&fixture.config, request),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn missing_or_conflicting_effective_rule_revokes_authorization() {
    let fixture = Fixture::new();
    let cases = [
        Vec::new(),
        vec![required_rule(None)],
        vec![required_rule(Some(APP_ID + 1))],
        vec![required_rule(Some(APP_ID)), required_rule(Some(APP_ID + 1))],
        vec![required_rule_with_policy(Some(APP_ID), false)],
        vec![
            required_rule(Some(APP_ID)),
            required_rule_with_policy(Some(APP_ID), false),
        ],
    ];
    for rules in cases {
        let mut data = fixture.data.clone();
        data.rules = rules;
        let snapshot = super::refresh::snapshot(&fixture.config, fixture.request(), &data).unwrap();
        assert_eq!(snapshot.state, ChangeState::AuthorizationRevoked);
        assert_eq!(snapshot.run.commits.candidate, fixture.candidate);
    }

    let mut malformed = fixture.data.clone();
    malformed.rules = vec![BranchRule {
        kind: "required_status_checks".to_owned(),
        parameters: Some(serde_json::json!({"unexpected": []})),
    }];
    assert_eq!(
        super::refresh::snapshot(&fixture.config, fixture.request(), &malformed),
        Err(ProviderError::InvalidResponse)
    );

    let mut unknown_state = fixture.data.clone();
    unknown_state.pull_request.state = "unknown".to_owned();
    unknown_state.rules.clear();
    assert_eq!(
        super::refresh::snapshot(&fixture.config, fixture.request(), &unknown_state),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn closed_and_provider_revocation_are_distinct() {
    let mut fixture = Fixture::new();
    fixture.data.pull_request.state = "closed".to_owned();
    let closed =
        super::refresh::snapshot(&fixture.config, fixture.request(), &fixture.data).unwrap();
    assert_eq!(closed.state, ChangeState::Closed);

    let client = Client {
        config: fixture.config.clone(),
        rest: FakeRest::failing(ProviderError::AuthorizationRevoked),
    };
    assert_eq!(
        client.refresh(fixture.request()),
        Err(ProviderError::AuthorizationRevoked)
    );
}

#[test]
fn unavailable_event_candidate_fails_closed() {
    let fixture = Fixture::new();
    let client = Client {
        config: fixture.config.clone(),
        rest: FakeRest::with_state(
            Ok(fixture.data.pull_request.clone()),
            Err(ProviderError::Unavailable),
            Vec::new(),
        ),
    };
    assert_eq!(
        client.refresh(fixture.request()),
        Err(ProviderError::Unavailable)
    );
}

#[test]
fn publication_reuses_only_one_exact_owned_check() {
    let fixture = Fixture::new();
    let publication = fixture.publication(CheckConclusion::Pass);
    let expected =
        created_from_decision(publication_decision(&fixture.config, &publication, &[]).unwrap());
    let exact = check_run(APP_ID, &expected);
    assert!(matches!(
        publication_decision(&fixture.config, &publication, std::slice::from_ref(&exact)).unwrap(),
        PublicationDecision::Reuse
    ));

    let mut changed = exact.clone();
    changed.conclusion = Some("failure".to_owned());
    assert_eq!(
        decision_error(&fixture, &publication, &[changed]),
        ProviderError::InvalidResponse
    );
    let mut missing_output = exact.clone();
    missing_output.output.summary = None;
    assert_eq!(
        decision_error(&fixture, &publication, &[missing_output]),
        ProviderError::InvalidResponse
    );
    assert_eq!(
        decision_error(&fixture, &publication, &[exact.clone(), exact.clone()]),
        ProviderError::InvalidResponse
    );

    let mut other_evaluation = exact.clone();
    other_evaluation.external_id = Some("evaluation-older".to_owned());
    assert!(matches!(
        publication_decision(
            &fixture.config,
            &publication,
            std::slice::from_ref(&other_evaluation)
        )
        .unwrap(),
        PublicationDecision::Create(_)
    ));
    assert!(matches!(
        publication_decision(&fixture.config, &publication, &[other_evaluation, exact]).unwrap(),
        PublicationDecision::Reuse
    ));
}

#[test]
fn publication_conclusions_and_create_response_are_exact() {
    let fixture = Fixture::new();
    let cases = [
        (CheckConclusion::Pass, "success", "pass"),
        (CheckConclusion::Block, "failure", "block"),
        (CheckConclusion::Superseded, "cancelled", "superseded"),
        (
            CheckConclusion::Unavailable(RunFailure::Timeout),
            "failure",
            "unavailable",
        ),
    ];
    for (conclusion, expected_conclusion, label) in cases {
        let publication = fixture.publication(conclusion);
        let expected = created_from_decision(
            publication_decision(&fixture.config, &publication, &[]).unwrap(),
        );
        assert_eq!(expected.conclusion, expected_conclusion);
        assert_eq!(expected.head_sha, publication.gate_commit.as_str());
        let run = &publication.run;
        let repository = &run.change.repository;
        let bindings = [
            format!("evaluation: {}", publication.evaluation_id),
            format!("conclusion: {label}"),
            format!(
                "provider: {}/{}",
                run.change.provider.namespace, run.change.provider.instance
            ),
            format!(
                "repository: {}/{}/{}",
                repository.host, repository.owner, repository.name
            ),
            format!("change: {}", run.change.change),
            format!(
                "provider-run: {}#{}",
                publication.provider_run.run_id,
                publication.provider_run.attempt.get()
            ),
            format!("gate-commit: {}", publication.gate_commit.as_str()),
            format!("candidate-ref: {}", run.refs.candidate.as_str()),
            format!("target-ref: {}", run.refs.target.as_str()),
            format!("default-ref: {}", run.refs.default_branch.as_str()),
            format!("base-commit: {}", run.commits.base.as_str()),
            format!("base-tree: {}", run.trees.base.as_str()),
            format!("candidate-commit: {}", run.commits.candidate.as_str()),
            format!("candidate-tree: {}", run.trees.candidate.as_str()),
            format!("plan: {}", publication.check.plan_digest),
            format!(
                "constraint: {}",
                publication.check.execution_constraint_digest
            ),
            format!(
                "report: {}",
                sha256(publication.report.as_deref().unwrap_or_default())
            ),
        ];
        for binding in bindings {
            assert!(
                expected.output.summary.lines().any(|line| line == binding),
                "missing summary binding: {binding}"
            );
        }
        if matches!(conclusion, CheckConclusion::Unavailable(_)) {
            assert!(expected.output.summary.contains("failure: timeout"));
        } else {
            assert!(!expected.output.summary.contains("\nfailure:"));
        }
        assert_eq!(
            validate_created(&fixture.config, &expected, &check_run(APP_ID, &expected)),
            Ok(())
        );
    }
}

#[test]
fn client_uses_the_fake_rest_seam_without_blind_create_retry() {
    let fixture = Fixture::new();
    let publication = fixture.publication(CheckConclusion::Pass);
    let expected =
        created_from_decision(publication_decision(&fixture.config, &publication, &[]).unwrap());
    let rest = FakeRest::with_runs(fixture.data.clone(), vec![check_run(APP_ID, &expected)]);
    let client = Client {
        config: fixture.config.clone(),
        rest,
    };
    assert_eq!(client.publish(fixture.request(), &publication), Ok(()));
    assert_eq!(client.rest.creates.load(Ordering::Relaxed), 0);
}

#[test]
fn publication_is_bound_before_provider_io() {
    let fixture = Fixture::new();
    let mut publication = fixture.publication(CheckConclusion::Pass);
    publication.provider_run.candidate_commit = oid('e');
    let rest = FakeRest::with_runs(fixture.data.clone(), Vec::new());
    let client = Client {
        config: fixture.config.clone(),
        rest,
    };

    assert_eq!(
        client.publish(fixture.request(), &publication),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(client.rest.checks.load(Ordering::Relaxed), 0);
    assert_eq!(client.rest.creates.load(Ordering::Relaxed), 0);
}

#[test]
fn authoritative_ref_drift_retires_a_cancelled_publication() {
    let fixture = Fixture::new();
    let target = BranchRef::new("refs/heads/release".to_owned()).unwrap();

    let mut passing = fixture.publication(CheckConclusion::Pass);
    passing.run.refs.target = target.clone();
    let passing_client = Client {
        config: fixture.config.clone(),
        rest: FakeRest::with_runs(fixture.data.clone(), Vec::new()),
    };
    assert_eq!(
        passing_client.publish(fixture.request(), &passing),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(passing_client.rest.checks.load(Ordering::Relaxed), 0);

    let mut cancelled = fixture.publication(CheckConclusion::Superseded);
    cancelled.run.refs.target = target;
    let cancelled_client = Client {
        config: fixture.config.clone(),
        rest: FakeRest::with_runs(fixture.data.clone(), Vec::new()),
    };
    assert_eq!(
        cancelled_client.publish(fixture.request(), &cancelled),
        Ok(())
    );
    assert_eq!(cancelled_client.rest.checks.load(Ordering::Relaxed), 0);
    assert_eq!(cancelled_client.rest.creates.load(Ordering::Relaxed), 0);
}

#[test]
fn moved_head_publication_never_lands_on_the_new_gate() {
    let fixture = Fixture::new();
    let moved = fixture.moved_data();
    let snapshot = super::refresh::snapshot(&fixture.config, fixture.request(), &moved).unwrap();
    assert_eq!(snapshot.state, ChangeState::Superseded);

    let mut publication = fixture.publication(CheckConclusion::Superseded);
    publication.gate_commit = snapshot.gate_commit;
    let client = Client {
        config: fixture.config.clone(),
        rest: FakeRest::with_state(
            Ok(moved.pull_request),
            Err(ProviderError::Unavailable),
            Vec::new(),
        ),
    };
    assert_eq!(client.publish(fixture.request(), &publication), Ok(()));
    assert_eq!(client.rest.checks.load(Ordering::Relaxed), 0);
    assert_eq!(client.rest.creates.load(Ordering::Relaxed), 0);
}

#[test]
fn timeout_configuration_bounds_the_whole_operation() {
    let second = std::time::Duration::from_secs(1);
    assert!(super::GitHubTimeouts::new(second, second).is_some());
    for invalid in [
        super::GitHubTimeouts::new(std::time::Duration::ZERO, second),
        super::GitHubTimeouts::new(second, std::time::Duration::ZERO),
        super::GitHubTimeouts::new(second, std::time::Duration::from_millis(999)),
        super::GitHubTimeouts::new(second, std::time::Duration::from_secs(31)),
    ] {
        assert!(invalid.is_none());
    }
}

struct Fixture {
    config: Config,
    change: ChangeLocator,
    candidate: Oid,
    data: RefreshData,
}

impl Fixture {
    fn new() -> Self {
        let provider = provider();
        let repository =
            RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap();
        let change = ChangeLocator {
            provider: provider.clone(),
            repository,
            change: change_id(),
        };
        let candidate = oid('b');
        Self {
            config: Config {
                provider,
                app_id: APP_ID,
                installation_id: INSTALLATION_ID,
                required_status_name: "amiss/provider".to_owned(),
            },
            change,
            candidate: candidate.clone(),
            data: refresh_data(&candidate),
        }
    }

    fn request(&self) -> GitHubPullRequest<'_> {
        GitHubPullRequest {
            change: &self.change,
            installation_id: INSTALLATION_ID,
            repository_id: 101,
            repository_owner: "acme",
            repository_name: "widget",
            pull_request_id: 4_201,
            number: 42,
            candidate_commit: &self.candidate,
        }
    }

    fn publication(&self, conclusion: CheckConclusion) -> Publication {
        let snapshot = super::refresh::snapshot(&self.config, self.request(), &self.data).unwrap();
        let digest = hb("amiss/controller-github-live-test", b"fixture");
        let integration = IntegrationId::new(INSTALLATION_ID.to_string()).unwrap();
        let provider_run = crate::provider_run(
            &integration,
            &self.change,
            &self.candidate,
            &snapshot.run.refs.candidate,
            &snapshot.run.refs.target,
        )
        .unwrap();
        let gate_commit = snapshot.gate_commit.clone();
        Publication {
            provider_run,
            evaluation_id: ControllerEvaluationId::new("evaluation-1".to_owned()).unwrap(),
            check: CheckBinding {
                plan_digest: digest,
                required_status_name: self.config.required_status_name.clone(),
                execution_constraint_digest: digest,
            },
            run: snapshot.run,
            gate_commit,
            conclusion,
            report: Some(br#"{"schema":"amiss/report"}"#.to_vec()),
        }
    }

    fn moved_data(&self) -> RefreshData {
        let mut data = self.data.clone();
        let current_head = oid('f');
        let current_tree = oid('1');
        let current_gate = oid('2');
        data.pull_request.head.sha = current_head.as_str().to_owned();
        data.pull_request.merge_commit_sha = Some(current_gate.as_str().to_owned());
        data.current_head = CommitRecord {
            sha: current_head.as_str().to_owned(),
            tree: current_tree.as_str().to_owned(),
        };
        data.gate = super::model::GateCommitRecord {
            sha: current_gate.as_str().to_owned(),
            tree: current_tree.as_str().to_owned(),
            parents: vec![
                oid('a').as_str().to_owned(),
                current_head.as_str().to_owned(),
            ],
        };
        data
    }
}

struct FakeRest {
    refresh: Mutex<Result<RefreshData, ProviderError>>,
    current: Mutex<Result<PullRequestRecord, ProviderError>>,
    runs: Vec<CheckRunRecord>,
    checks: AtomicUsize,
    creates: AtomicUsize,
}

impl FakeRest {
    fn failing(error: ProviderError) -> Self {
        Self {
            refresh: Mutex::new(Err(error)),
            current: Mutex::new(Err(error)),
            runs: Vec::new(),
            checks: AtomicUsize::new(0),
            creates: AtomicUsize::new(0),
        }
    }

    fn with_runs(data: RefreshData, runs: Vec<CheckRunRecord>) -> Self {
        let current = data.pull_request.clone();
        Self::with_state(Ok(current), Ok(data), runs)
    }

    fn with_state(
        current: Result<PullRequestRecord, ProviderError>,
        refresh: Result<RefreshData, ProviderError>,
        runs: Vec<CheckRunRecord>,
    ) -> Self {
        Self {
            refresh: Mutex::new(refresh),
            current: Mutex::new(current),
            runs,
            checks: AtomicUsize::new(0),
            creates: AtomicUsize::new(0),
        }
    }
}

impl GitHubRest for FakeRest {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        OperationDeadline::after(std::time::Duration::from_secs(30))
    }

    fn pull_request(
        &self,
        _pull_request: GitHubPullRequest<'_>,
        _deadline: OperationDeadline,
    ) -> Result<PullRequestRecord, ProviderError> {
        self.current.lock().unwrap().clone()
    }

    fn refresh_data(
        &self,
        _pull_request: GitHubPullRequest<'_>,
        _deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError> {
        self.refresh.lock().unwrap().clone()
    }

    fn check_runs(
        &self,
        _pull_request: GitHubPullRequest<'_>,
        _head_sha: &Oid,
        _app_id: u64,
        _name: &str,
        _deadline: OperationDeadline,
    ) -> Result<Vec<CheckRunRecord>, ProviderError> {
        self.checks.fetch_add(1, Ordering::Relaxed);
        Ok(self.runs.clone())
    }

    fn create_check_run(
        &self,
        _pull_request: GitHubPullRequest<'_>,
        check: &CreateCheckRun,
        _deadline: OperationDeadline,
    ) -> Result<CheckRunRecord, ProviderError> {
        self.creates.fetch_add(1, Ordering::Relaxed);
        Ok(check_run(APP_ID, check))
    }
}

fn refresh_data(candidate: &Oid) -> RefreshData {
    let base_repository = PullRepositoryRecord {
        id: 101,
        name: "widget".to_owned(),
        full_name: "Acme/Widget".to_owned(),
        owner: OwnerRecord {
            login: "Acme".to_owned(),
        },
    };
    RefreshData {
        repository: RepositoryRecord {
            id: 101,
            name: "Widget".to_owned(),
            full_name: "Acme/Widget".to_owned(),
            owner: OwnerRecord {
                login: "Acme".to_owned(),
            },
            default_branch: "main".to_owned(),
        },
        pull_request: PullRequestRecord {
            id: 4_201,
            number: 42,
            state: "open".to_owned(),
            mergeable: Some(true),
            merge_commit_sha: Some(oid('e').as_str().to_owned()),
            head: PullRefRecord {
                sha: candidate.as_str().to_owned(),
                branch: "topic".to_owned(),
                repo: Some(PullRepositoryRecord {
                    id: 202,
                    name: "widget-fork".to_owned(),
                    full_name: "Contributor/widget-fork".to_owned(),
                    owner: OwnerRecord {
                        login: "Contributor".to_owned(),
                    },
                }),
            },
            base: PullRefRecord {
                sha: oid('a').as_str().to_owned(),
                branch: "main".to_owned(),
                repo: Some(base_repository),
            },
        },
        target: CommitRecord {
            sha: oid('a').as_str().to_owned(),
            tree: oid('c').as_str().to_owned(),
        },
        candidate: CommitRecord {
            sha: candidate.as_str().to_owned(),
            tree: oid('d').as_str().to_owned(),
        },
        current_head: CommitRecord {
            sha: candidate.as_str().to_owned(),
            tree: oid('d').as_str().to_owned(),
        },
        gate: super::model::GateCommitRecord {
            sha: oid('e').as_str().to_owned(),
            tree: oid('d').as_str().to_owned(),
            parents: vec![oid('a').as_str().to_owned(), candidate.as_str().to_owned()],
        },
        rules: vec![required_rule(Some(APP_ID))],
    }
}

fn required_rule(integration_id: Option<u64>) -> BranchRule {
    required_rule_with_policy(integration_id, true)
}

fn required_rule_with_policy(integration_id: Option<u64>, strict: bool) -> BranchRule {
    BranchRule {
        kind: "required_status_checks".to_owned(),
        parameters: Some(serde_json::json!({
            "strict_required_status_checks_policy": strict,
            "required_status_checks": [{
                "context": "amiss/provider",
                "integration_id": integration_id
            }]
        })),
    }
}

fn check_run(app_id: u64, expected: &CreateCheckRun) -> CheckRunRecord {
    CheckRunRecord {
        id: 81,
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

fn created_from_decision(decision: PublicationDecision) -> CreateCheckRun {
    match decision {
        PublicationDecision::Create(expected) => Some(expected),
        PublicationDecision::Reuse => None,
    }
    .unwrap()
}

fn decision_error(
    fixture: &Fixture,
    publication: &Publication,
    runs: &[CheckRunRecord],
) -> ProviderError {
    publication_decision(&fixture.config, publication, runs)
        .err()
        .unwrap()
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.com".to_owned()).unwrap(),
    }
}

fn change_id() -> ChangeId {
    OpaqueId::new("repository/101/pull/4201/number/42".to_owned()).unwrap()
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}
