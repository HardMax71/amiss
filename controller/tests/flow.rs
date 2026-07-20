#![expect(
    clippy::unwrap_used,
    reason = "fixed test fixtures and poison-free test mutexes must fail loudly"
)]

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use amiss_controller::{
    AdapterRegistry, AuthenticatedDelivery, ChangeId, ChangeLocator, ChangeSnapshot, ChangeState,
    CheckConclusion, Controller, ControllerError, ControllerEvaluationId, DeliveryClaim,
    DeliveryId, DeliveryIdentity, DeliveryLedger, Evaluation, HandleOutcome, IntegrationId,
    ProviderAdapter, ProviderError, ProviderErrorKind, ProviderIdentity, ProviderInstance,
    ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, Publication,
    RunFailure, RunIdentity, RunRequest, Runner, RunnerOutcome, UntrustedDelivery,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::report::MACHINE_JSON_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LedgerError;

impl fmt::Display for LedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("test ledger error")
    }
}

impl std::error::Error for LedgerError {}

#[derive(Clone)]
struct LedgerRow {
    evaluation_id: ControllerEvaluationId,
    complete: bool,
}

#[derive(Default)]
struct MemoryLedger {
    rows: BTreeMap<DeliveryIdentity, LedgerRow>,
}

impl DeliveryLedger for MemoryLedger {
    type Error = LedgerError;

    fn claim(&mut self, delivery: &DeliveryIdentity) -> Result<DeliveryClaim, Self::Error> {
        if let Some(row) = self.rows.get(delivery) {
            return if row.complete {
                Ok(DeliveryClaim::Duplicate)
            } else {
                Ok(DeliveryClaim::Execute(row.evaluation_id.clone()))
            };
        }
        let evaluation_id = ControllerEvaluationId::new("evaluation-01".to_owned()).unwrap();
        self.rows.insert(
            delivery.clone(),
            LedgerRow {
                evaluation_id: evaluation_id.clone(),
                complete: false,
            },
        );
        Ok(DeliveryClaim::Execute(evaluation_id))
    }

    fn complete(
        &mut self,
        delivery: &DeliveryIdentity,
        evaluation_id: &ControllerEvaluationId,
    ) -> Result<(), Self::Error> {
        let Some(row) = self.rows.get_mut(delivery) else {
            return Err(LedgerError);
        };
        if row.evaluation_id != *evaluation_id {
            return Err(LedgerError);
        }
        row.complete = true;
        Ok(())
    }
}

struct FakeRunner {
    outcomes: VecDeque<RunnerOutcome>,
    requests: Vec<RunRequest>,
}

impl FakeRunner {
    fn new(outcome: RunnerOutcome) -> Self {
        Self {
            outcomes: VecDeque::from([outcome]),
            requests: Vec::new(),
        }
    }
}

impl Runner for FakeRunner {
    fn run(&mut self, request: &RunRequest) -> RunnerOutcome {
        self.requests.push(request.clone());
        self.outcomes
            .pop_front()
            .unwrap_or(RunnerOutcome::Unavailable)
    }
}

struct FakeAdapter {
    namespace: ProviderNamespace,
    authenticated: AuthenticatedDelivery,
    refreshes: Mutex<VecDeque<Result<ChangeSnapshot, ProviderError>>>,
    publications: Mutex<Vec<Publication>>,
    authentication_count: AtomicUsize,
    refresh_count: AtomicUsize,
}

impl FakeAdapter {
    fn new(
        authenticated: AuthenticatedDelivery,
        refreshes: impl IntoIterator<Item = Result<ChangeSnapshot, ProviderError>>,
    ) -> Self {
        Self {
            namespace: authenticated.identity.provider.namespace.clone(),
            authenticated,
            refreshes: Mutex::new(refreshes.into_iter().collect()),
            publications: Mutex::new(Vec::new()),
            authentication_count: AtomicUsize::new(0),
            refresh_count: AtomicUsize::new(0),
        }
    }

    fn publications(&self) -> Vec<Publication> {
        self.publications.lock().unwrap().clone()
    }
}

impl ProviderAdapter for FakeAdapter {
    fn namespace(&self) -> &ProviderNamespace {
        &self.namespace
    }

    fn authenticate(
        &self,
        _delivery: UntrustedDelivery<'_>,
    ) -> Result<AuthenticatedDelivery, ProviderError> {
        self.authentication_count.fetch_add(1, Ordering::Relaxed);
        Ok(self.authenticated.clone())
    }

    fn refresh(&self, _delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        self.refresh_count.fetch_add(1, Ordering::Relaxed);
        self.refreshes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(ProviderError::new(ProviderErrorKind::Unavailable)))
    }

    fn publish(
        &self,
        _delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.publications.lock().unwrap().push(publication.clone());
        Ok(())
    }
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("forgejo".to_owned()).unwrap(),
        instance: ProviderInstance::new("forge.example.test".to_owned()).unwrap(),
    }
}

fn repository(name: &str) -> RepositoryIdentity {
    RepositoryIdentity::new(
        "forge.example.test".to_owned(),
        "owner".to_owned(),
        name.to_owned(),
    )
    .unwrap()
}

fn locator(provider: &ProviderIdentity, repository: RepositoryIdentity) -> ChangeLocator {
    ChangeLocator {
        provider: provider.clone(),
        repository,
        change: ChangeId::new("42".to_owned()).unwrap(),
    }
}

fn delivery(
    provider: &ProviderIdentity,
    change: ChangeLocator,
    candidate_commit: char,
) -> AuthenticatedDelivery {
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("installation-7".to_owned()).unwrap(),
            delivery: DeliveryId::new("delivery-9".to_owned()).unwrap(),
        },
        change,
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("provider-run-11".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            oid(candidate_commit),
        )
        .unwrap(),
    }
}

fn oid(byte: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, byte.to_string().repeat(40)).unwrap()
}

fn run(change: ChangeLocator, candidate_commit: char, candidate_tree: char) -> RunIdentity {
    run_with_resolution(
        change,
        candidate_commit,
        candidate_tree,
        ForgeDialect::Gitea,
        "refs/heads/main",
    )
}

fn run_with_resolution(
    change: ChangeLocator,
    candidate_commit: char,
    candidate_tree: char,
    forge: ForgeDialect,
    default_branch_ref: &str,
) -> RunIdentity {
    RunIdentity::new(
        change,
        forge,
        BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
        BranchRef::new(default_branch_ref.to_owned()).unwrap(),
        ObjectFormat::Sha1,
        oid('a'),
        oid(candidate_commit),
        oid('c'),
        oid(candidate_tree),
    )
    .unwrap()
}

fn snapshot(state: ChangeState, run: RunIdentity) -> ChangeSnapshot {
    ChangeSnapshot { state, run }
}

fn complete(run: &RunIdentity) -> RunnerOutcome {
    RunnerOutcome::Complete {
        identity: Box::new(run.clone()),
        evaluation: Evaluation::Pass,
        report: br#"{"schema":"amiss/report"}"#.to_vec(),
    }
}

fn input(provider: &ProviderIdentity) -> UntrustedDelivery<'_> {
    UntrustedDelivery {
        expected_provider: provider,
        received_at_unix_seconds: 1_800_000_000,
        headers: &[],
        body: br#"{"event":"change"}"#,
    }
}

fn controller(
    adapter: Arc<FakeAdapter>,
    outcome: RunnerOutcome,
) -> Controller<MemoryLedger, FakeRunner> {
    let mut registry = AdapterRegistry::new();
    registry.register(adapter).unwrap();
    Controller::new(registry, MemoryLedger::default(), FakeRunner::new(outcome))
}

#[test]
fn successful_flow_binds_run_rechecks_and_publishes() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let authenticated = delivery(&provider, change, 'b');
    let adapter = Arc::new(FakeAdapter::new(
        authenticated.clone(),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Published(CheckConclusion::Pass)
    );
    assert_eq!(adapter.authentication_count.load(Ordering::Relaxed), 1);
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 2);
    assert_eq!(controller.runner().requests.len(), 1);
    assert_eq!(controller.runner().requests[0].run, run);
    assert_eq!(
        controller.runner().requests[0].provider_run,
        authenticated.provider_run
    );
    let publications = adapter.publications();
    assert_eq!(publications.len(), 1);
    assert_eq!(publications[0].conclusion, CheckConclusion::Pass);
    assert!(publications[0].report.is_some());
}

#[test]
fn completed_delivery_is_a_duplicate_without_another_run() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert!(matches!(
        controller.handle(input(&provider)),
        Ok(HandleOutcome::Published(CheckConclusion::Pass))
    ));
    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Duplicate
    );
    assert_eq!(controller.runner().requests.len(), 1);
    assert_eq!(adapter.publications().len(), 1);
}

#[test]
fn incomplete_claim_resumes_after_a_transient_refresh_failure() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Err(ProviderError::new(ProviderErrorKind::Unavailable)),
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run.clone())),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::Provider(_))
    ));
    assert!(matches!(
        controller.handle(input(&provider)),
        Ok(HandleOutcome::Published(CheckConclusion::Pass))
    ));
    assert_eq!(
        controller.runner().requests[0].evaluation_id.as_str(),
        "evaluation-01"
    );
}

#[test]
fn resumed_delivery_cannot_follow_the_changes_new_head() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let original = run(change.clone(), 'b', 'd');
    let moved = run(change.clone(), 'e', 'f');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Err(ProviderError::new(ProviderErrorKind::Unavailable)),
            Ok(snapshot(ChangeState::Active, moved)),
        ],
    ));
    let mut controller = controller(Arc::clone(&adapter), complete(&original));

    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::Provider(_))
    ));
    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::WrongProviderRun)
    ));
    assert!(controller.runner().requests.is_empty());
    assert!(adapter.publications().is_empty());
}

#[test]
fn authenticated_provider_must_match_the_routed_instance() {
    let actual = provider();
    let mut expected = actual.clone();
    expected.instance = ProviderInstance::new("other.example.test".to_owned()).unwrap();
    let change = locator(&actual, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(delivery(&actual, change, 'b'), []));
    let mut controller = controller(adapter, complete(&run));

    assert!(matches!(
        controller.handle(input(&expected)),
        Err(ControllerError::WrongAuthenticatedProvider)
    ));
    assert!(controller.runner().requests.is_empty());
}

#[test]
fn refresh_cannot_substitute_another_repository() {
    let provider = provider();
    let authenticated_change = locator(&provider, repository("amiss"));
    let wrong_change = locator(&provider, repository("other"));
    let wrong_run = run(wrong_change, 'b', 'd');
    let expected_run = run(authenticated_change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, authenticated_change, 'b'),
        [Ok(snapshot(ChangeState::Active, wrong_run))],
    ));
    let mut controller = controller(adapter, complete(&expected_run));

    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::WrongChangeIdentity)
    ));
    assert!(controller.runner().requests.is_empty());
}

#[test]
fn runner_commit_and_tree_mismatches_fail_closed() {
    let cases = [
        ('e', 'd', RunFailure::WrongIdentity),
        ('b', 'e', RunFailure::WrongTree),
    ];
    for (candidate_commit, candidate_tree, failure) in cases {
        let provider = provider();
        let change = locator(&provider, repository("amiss"));
        let expected = run(change.clone(), 'b', 'd');
        let wrong = run(change.clone(), candidate_commit, candidate_tree);
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change, 'b'),
            [
                Ok(snapshot(ChangeState::Active, expected.clone())),
                Ok(snapshot(ChangeState::Active, expected.clone())),
            ],
        ));
        let mut controller = controller(Arc::clone(&adapter), complete(&wrong));

        assert_eq!(
            controller.handle(input(&provider)).unwrap(),
            HandleOutcome::Published(CheckConclusion::Unavailable(failure))
        );
        assert_eq!(adapter.publications()[0].report, None);
    }
}

#[test]
fn runner_wrong_resolution_identity_fails_closed() {
    let cases = [
        (ForgeDialect::Github, "refs/heads/main"),
        (ForgeDialect::Gitea, "refs/heads/trunk"),
    ];
    for (forge, default_branch_ref) in cases {
        let provider = provider();
        let change = locator(&provider, repository("amiss"));
        let expected = run(change.clone(), 'b', 'd');
        let wrong = run_with_resolution(change.clone(), 'b', 'd', forge, default_branch_ref);
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change, 'b'),
            [
                Ok(snapshot(ChangeState::Active, expected.clone())),
                Ok(snapshot(ChangeState::Active, expected.clone())),
            ],
        ));
        let mut controller = controller(adapter, complete(&wrong));

        assert_eq!(
            controller.handle(input(&provider)).unwrap(),
            HandleOutcome::Published(CheckConclusion::Unavailable(RunFailure::WrongIdentity))
        );
    }
}

#[test]
fn provider_supersession_is_published_for_the_original_candidate() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let initial = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, initial.clone())),
            Ok(snapshot(ChangeState::Superseded, initial.clone())),
        ],
    ));
    let mut controller = controller(adapter, complete(&initial));

    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Published(CheckConclusion::Superseded)
    );
}

#[test]
fn revoked_authorization_overrides_a_successful_runner() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::AuthorizationRevoked, run.clone())),
        ],
    ));
    let mut controller = controller(adapter, complete(&run));

    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Published(CheckConclusion::Unavailable(
            RunFailure::AuthorizationRevoked
        ))
    );
}

#[test]
fn missing_timeout_and_tampered_results_all_fail_closed() {
    let cases = [
        (RunnerOutcome::MissingOutput, RunFailure::MissingOutput),
        (RunnerOutcome::TimedOut, RunFailure::Timeout),
        (RunnerOutcome::TamperedRuntime, RunFailure::TamperedRuntime),
    ];
    for (outcome, failure) in cases {
        let provider = provider();
        let change = locator(&provider, repository("amiss"));
        let run = run(change.clone(), 'b', 'd');
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change, 'b'),
            [
                Ok(snapshot(ChangeState::Active, run.clone())),
                Ok(snapshot(ChangeState::Active, run.clone())),
            ],
        ));
        let mut controller = controller(adapter, outcome);

        assert_eq!(
            controller.handle(input(&provider)).unwrap(),
            HandleOutcome::Published(CheckConclusion::Unavailable(failure))
        );
    }
}

#[test]
fn oversized_report_is_not_accepted_for_publication() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let oversized = usize::try_from(MACHINE_JSON_BYTES).unwrap() + 1;
    let outcome = RunnerOutcome::Complete {
        identity: Box::new(run.clone()),
        evaluation: Evaluation::Pass,
        report: vec![b'x'; oversized],
    };
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Ok(snapshot(ChangeState::Active, run.clone())),
            Ok(snapshot(ChangeState::Active, run)),
        ],
    ));
    let mut controller = controller(adapter, outcome);

    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Published(CheckConclusion::Unavailable(RunFailure::OversizedOutput))
    );
}

#[test]
fn run_identity_rejects_oids_from_another_object_format() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let invalid = RunIdentity::new(
        change,
        ForgeDialect::Gitea,
        BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
        BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        ObjectFormat::Sha256,
        oid('a'),
        oid('b'),
        oid('c'),
        oid('d'),
    );

    assert!(invalid.is_none());
}
