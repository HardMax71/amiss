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
    DeliveryId, DeliveryIdentity, DeliveryLease, DeliveryLedger, Evaluation, HandleOutcome,
    HeartbeatOutcome, IntegrationId, LeaseCompletion, LeaseFence, LeaseRenewal, OidPair,
    ProviderAdapter, ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, Publication, RunFailure, RunHeartbeat,
    RunIdentity, RunRefs, RunRequest, Runner, RunnerOutcome, StageOutcome, StagedPublication,
    UntrustedDelivery,
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
    binding: AuthenticatedDelivery,
    lease: DeliveryLease,
    staged: Option<StagedPublication>,
    complete: bool,
}

#[derive(Default)]
struct MemoryLedger {
    rows: BTreeMap<DeliveryIdentity, LedgerRow>,
    renewal_count: usize,
}

impl DeliveryLedger for MemoryLedger {
    type Error = LedgerError;

    fn claim(&mut self, delivery: &AuthenticatedDelivery) -> Result<DeliveryClaim, Self::Error> {
        if let Some(row) = self.rows.get(&delivery.identity) {
            if row.binding != *delivery {
                return Ok(DeliveryClaim::BindingConflict);
            }
            return if row.complete {
                Ok(DeliveryClaim::Duplicate {
                    evaluation_id: row.lease.evaluation_id.clone(),
                })
            } else if let Some(staged) = &row.staged {
                Ok(DeliveryClaim::Publish(staged.clone()))
            } else {
                Ok(DeliveryClaim::Execute(row.lease.clone()))
            };
        }
        let lease = lease();
        self.rows.insert(
            delivery.identity.clone(),
            LedgerRow {
                binding: delivery.clone(),
                lease: lease.clone(),
                staged: None,
                complete: false,
            },
        );
        Ok(DeliveryClaim::Execute(lease))
    }

    fn renew(
        &mut self,
        delivery: &AuthenticatedDelivery,
        lease: &DeliveryLease,
    ) -> Result<LeaseRenewal, Self::Error> {
        self.renewal_count = self.renewal_count.saturating_add(1);
        let Some(row) = self.rows.get_mut(&delivery.identity) else {
            return Ok(LeaseRenewal::Lost);
        };
        if row.binding == *delivery && row.lease == *lease && row.staged.is_none() && !row.complete
        {
            row.lease.expires_at_unix_millis = row.lease.expires_at_unix_millis.saturating_add(1);
            Ok(LeaseRenewal::Renewed(row.lease.clone()))
        } else {
            Ok(LeaseRenewal::Lost)
        }
    }

    fn complete(
        &mut self,
        delivery: &AuthenticatedDelivery,
        staged: &StagedPublication,
    ) -> Result<LeaseCompletion, Self::Error> {
        let Some(row) = self.rows.get_mut(&delivery.identity) else {
            return Ok(LeaseCompletion::Lost);
        };
        if row.binding != *delivery || row.staged.as_ref() != Some(staged) {
            return Ok(LeaseCompletion::Lost);
        }
        row.complete = true;
        Ok(LeaseCompletion::Completed)
    }

    fn stage(
        &mut self,
        delivery: &AuthenticatedDelivery,
        lease: &DeliveryLease,
        publication: &Publication,
    ) -> Result<StageOutcome, Self::Error> {
        let Some(row) = self.rows.get_mut(&delivery.identity) else {
            return Ok(StageOutcome::Lost);
        };
        if row.binding != *delivery || row.lease != *lease || row.complete {
            return Ok(StageOutcome::Lost);
        }
        let staged = StagedPublication {
            evaluation_id: lease.evaluation_id.clone(),
            fence: lease.fence,
            publication: Box::new(publication.clone()),
        };
        match &row.staged {
            Some(existing) if *existing == staged => Ok(StageOutcome::Staged(existing.clone())),
            Some(_) => Ok(StageOutcome::Lost),
            None => {
                row.staged = Some(staged.clone());
                Ok(StageOutcome::Staged(staged))
            }
        }
    }
}

struct ScriptedLedger {
    claim: Option<DeliveryClaim>,
    renewals: VecDeque<Result<LeaseRenewal, LedgerError>>,
    stage: Option<StageOutcome>,
    completion: LeaseCompletion,
}

impl DeliveryLedger for ScriptedLedger {
    type Error = LedgerError;

    fn claim(&mut self, _delivery: &AuthenticatedDelivery) -> Result<DeliveryClaim, Self::Error> {
        self.claim.take().ok_or(LedgerError)
    }

    fn renew(
        &mut self,
        _delivery: &AuthenticatedDelivery,
        _lease: &DeliveryLease,
    ) -> Result<LeaseRenewal, Self::Error> {
        match self.renewals.pop_front() {
            Some(result) => result,
            None => Err(LedgerError),
        }
    }

    fn complete(
        &mut self,
        _delivery: &AuthenticatedDelivery,
        _staged: &StagedPublication,
    ) -> Result<LeaseCompletion, Self::Error> {
        Ok(self.completion)
    }

    fn stage(
        &mut self,
        _delivery: &AuthenticatedDelivery,
        _lease: &DeliveryLease,
        _publication: &Publication,
    ) -> Result<StageOutcome, Self::Error> {
        self.stage.take().ok_or(LedgerError)
    }
}

struct FakeRunner {
    outcomes: VecDeque<RunnerOutcome>,
    requests: Vec<RunRequest>,
    heartbeat_renewals: usize,
    heartbeat_deadlines: Vec<i64>,
}

impl FakeRunner {
    fn new(outcome: RunnerOutcome) -> Self {
        Self {
            outcomes: VecDeque::from([outcome]),
            requests: Vec::new(),
            heartbeat_renewals: 0,
            heartbeat_deadlines: Vec::new(),
        }
    }
}

impl Runner for FakeRunner {
    fn run(&mut self, request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome {
        self.requests.push(request.clone());
        self.heartbeat_deadlines
            .push(heartbeat.expires_at_unix_millis());
        for _ in 0..self.heartbeat_renewals {
            match heartbeat.renew() {
                HeartbeatOutcome::Renewed {
                    expires_at_unix_millis,
                } => self.heartbeat_deadlines.push(expires_at_unix_millis),
                HeartbeatOutcome::Stop => return RunnerOutcome::Unavailable,
            }
        }
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
    publish_results: Mutex<VecDeque<Result<(), ProviderError>>>,
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
            publish_results: Mutex::new(VecDeque::new()),
            authentication_count: AtomicUsize::new(0),
            refresh_count: AtomicUsize::new(0),
        }
    }

    fn publications(&self) -> Vec<Publication> {
        self.publications.lock().unwrap().clone()
    }

    fn with_publish_results(
        self,
        results: impl IntoIterator<Item = Result<(), ProviderError>>,
    ) -> Self {
        *self.publish_results.lock().unwrap() = results.into_iter().collect();
        self
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
            .unwrap_or(Err(ProviderError::Unavailable))
    }

    fn publish(
        &self,
        _delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.publications.lock().unwrap().push(publication.clone());
        self.publish_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(()))
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
        RunRefs {
            forge,
            candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
            target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            default_branch: BranchRef::new(default_branch_ref.to_owned()).unwrap(),
        },
        ObjectFormat::Sha1,
        OidPair {
            base: oid('a'),
            candidate: oid(candidate_commit),
        },
        OidPair {
            base: oid('c'),
            candidate: oid(candidate_tree),
        },
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
    controller_with_ledger(adapter, MemoryLedger::default(), outcome)
}

fn controller_with_ledger<L: DeliveryLedger>(
    adapter: Arc<FakeAdapter>,
    ledger: L,
    outcome: RunnerOutcome,
) -> Controller<L, FakeRunner> {
    let mut registry = AdapterRegistry::new();
    registry.register(adapter).unwrap();
    Controller::new(registry, ledger, FakeRunner::new(outcome))
}

fn lease() -> DeliveryLease {
    DeliveryLease {
        evaluation_id: ControllerEvaluationId::new("evaluation-01".to_owned()).unwrap(),
        fence: LeaseFence::new(1).unwrap(),
        expires_at_unix_millis: 1_800_000_100_000,
    }
}

fn renewal_script(
    outcomes: impl IntoIterator<Item = LeaseRenewal>,
) -> VecDeque<Result<LeaseRenewal, LedgerError>> {
    outcomes.into_iter().map(Ok).collect()
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
    controller.runner.heartbeat_renewals = 2;

    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Published(CheckConclusion::Pass)
    );
    assert_eq!(adapter.authentication_count.load(Ordering::Relaxed), 1);
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 2);
    assert_eq!(controller.runner.requests.len(), 1);
    assert_eq!(controller.ledger.renewal_count, 5);
    assert_eq!(
        controller.runner.heartbeat_deadlines,
        vec![1_800_000_100_001, 1_800_000_100_002, 1_800_000_100_003,]
    );
    assert_eq!(controller.runner.requests[0].run, run);
    assert_eq!(
        controller.runner.requests[0].provider_run,
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
    assert!(matches!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Duplicate { evaluation_id }
            if evaluation_id.as_str() == "evaluation-01"
    ));
    assert_eq!(controller.runner.requests.len(), 1);
    assert_eq!(adapter.publications().len(), 1);
}

#[test]
fn a_staged_publication_retries_without_another_run() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(
        FakeAdapter::new(
            delivery(&provider, change, 'b'),
            [
                Ok(snapshot(ChangeState::Active, run.clone())),
                Ok(snapshot(ChangeState::Active, run.clone())),
            ],
        )
        .with_publish_results([Err(ProviderError::Unavailable), Ok(())]),
    );
    let mut controller = controller(Arc::clone(&adapter), complete(&run));

    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::Publish(ProviderError::Unavailable))
    ));
    let first = adapter.publications();
    assert_eq!(first.len(), 1);

    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Published(CheckConclusion::Pass)
    );
    let retried = adapter.publications();
    assert_eq!(retried.len(), 2);
    assert_eq!(retried.first(), retried.get(1));
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 2);
    assert_eq!(controller.runner.requests.len(), 1);

    assert!(matches!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::Duplicate { .. }
    ));
    assert_eq!(adapter.publications().len(), 2);
}

#[test]
fn a_live_lease_is_an_expected_in_progress_outcome() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(delivery(&provider, change, 'b'), []));
    let expected_lease = lease();
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Busy {
            evaluation_id: expected_lease.evaluation_id.clone(),
            retry_at_unix_millis: expected_lease.expires_at_unix_millis,
        }),
        renewals: VecDeque::new(),
        stage: None,
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert_eq!(
        controller.handle(input(&provider)).unwrap(),
        HandleOutcome::InProgress {
            evaluation_id: expected_lease.evaluation_id,
            retry_at_unix_millis: expected_lease.expires_at_unix_millis,
        }
    );
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 0);
    assert!(controller.runner.requests.is_empty());
    assert!(adapter.publications().is_empty());
}

#[test]
fn a_conflicting_delivery_binding_fails_before_refresh() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(delivery(&provider, change, 'b'), []));
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::BindingConflict),
        renewals: VecDeque::new(),
        stage: None,
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::DeliveryBindingConflict)
    ));
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 0);
    assert!(controller.runner.requests.is_empty());
    assert!(adapter.publications().is_empty());
}

#[test]
fn renewal_failure_during_or_after_a_run_stops_before_publication() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let expected_lease = lease();
    let changed_fence = DeliveryLease {
        fence: LeaseFence::new(2).unwrap(),
        ..expected_lease.clone()
    };
    for (heartbeat_renewals, failure) in [
        (0, Ok(LeaseRenewal::Lost)),
        (1, Ok(LeaseRenewal::Lost)),
        (1, Err(LedgerError)),
        (1, Ok(LeaseRenewal::Renewed(changed_fence))),
    ] {
        let expect_ledger_error = failure.is_err();
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change.clone(), 'b'),
            [Ok(snapshot(ChangeState::Active, run.clone()))],
        ));
        let ledger = ScriptedLedger {
            claim: Some(DeliveryClaim::Execute(expected_lease.clone())),
            renewals: VecDeque::from([Ok(LeaseRenewal::Renewed(expected_lease.clone())), failure]),
            stage: None,
            completion: LeaseCompletion::Lost,
        };
        let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));
        controller.runner.heartbeat_renewals = heartbeat_renewals;

        let result = controller.handle(input(&provider));
        if expect_ledger_error {
            assert!(matches!(result, Err(ControllerError::Ledger(LedgerError))));
        } else {
            assert!(matches!(result, Err(ControllerError::LeaseLost)));
        }
        assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 1);
        assert_eq!(controller.runner.requests.len(), 1);
        assert!(adapter.publications().is_empty());
    }
}

#[test]
fn a_publication_must_be_staged_under_the_live_fence() {
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
    let expected = lease();
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Execute(expected.clone())),
        renewals: renewal_script([
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected),
        ]),
        stage: Some(StageOutcome::Lost),
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::LeaseLost)
    ));
    assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 2);
    assert_eq!(controller.runner.requests.len(), 1);
    assert!(adapter.publications().is_empty());
}

#[test]
fn a_lost_completion_record_is_distinct_after_publication() {
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
    let expected = lease();
    let staged = StagedPublication {
        evaluation_id: expected.evaluation_id.clone(),
        fence: expected.fence,
        publication: Box::new(Publication {
            provider_run: authenticated.provider_run,
            evaluation_id: expected.evaluation_id.clone(),
            run: run.clone(),
            conclusion: CheckConclusion::Pass,
            report: Some(br#"{"schema":"amiss/report"}"#.to_vec()),
        }),
    };
    let ledger = ScriptedLedger {
        claim: Some(DeliveryClaim::Execute(expected.clone())),
        renewals: renewal_script([
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected.clone()),
            LeaseRenewal::Renewed(expected),
        ]),
        stage: Some(StageOutcome::Staged(staged)),
        completion: LeaseCompletion::Lost,
    };
    let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

    assert!(matches!(
        controller.handle(input(&provider)),
        Err(ControllerError::CompletionLost)
    ));
    assert_eq!(adapter.publications().len(), 1);
}

#[test]
fn a_ledger_cannot_change_the_lease_during_renewal() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let expected = lease();
    let changed_evaluation = DeliveryLease {
        evaluation_id: ControllerEvaluationId::new("evaluation-02".to_owned()).unwrap(),
        ..expected.clone()
    };
    let changed_fence = DeliveryLease {
        fence: LeaseFence::new(2).unwrap(),
        ..expected.clone()
    };
    let shortened = DeliveryLease {
        expires_at_unix_millis: expected.expires_at_unix_millis - 1,
        ..expected.clone()
    };

    for changed in [changed_evaluation, changed_fence, shortened] {
        let adapter = Arc::new(FakeAdapter::new(
            delivery(&provider, change.clone(), 'b'),
            [Ok(snapshot(ChangeState::Active, run.clone()))],
        ));
        let ledger = ScriptedLedger {
            claim: Some(DeliveryClaim::Execute(expected.clone())),
            renewals: renewal_script([LeaseRenewal::Renewed(changed)]),
            stage: None,
            completion: LeaseCompletion::Lost,
        };
        let mut controller = controller_with_ledger(Arc::clone(&adapter), ledger, complete(&run));

        assert!(matches!(
            controller.handle(input(&provider)),
            Err(ControllerError::LeaseLost)
        ));
        assert_eq!(adapter.refresh_count.load(Ordering::Relaxed), 1);
        assert!(controller.runner.requests.is_empty());
        assert!(adapter.publications().is_empty());
    }
}

#[test]
fn incomplete_claim_resumes_after_a_transient_refresh_failure() {
    let provider = provider();
    let change = locator(&provider, repository("amiss"));
    let run = run(change.clone(), 'b', 'd');
    let adapter = Arc::new(FakeAdapter::new(
        delivery(&provider, change, 'b'),
        [
            Err(ProviderError::Unavailable),
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
        controller.runner.requests[0].evaluation_id.as_str(),
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
            Err(ProviderError::Unavailable),
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
    assert!(controller.runner.requests.is_empty());
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
    assert!(controller.runner.requests.is_empty());
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
    assert!(controller.runner.requests.is_empty());
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
        RunRefs {
            forge: ForgeDialect::Gitea,
            candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
            target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
        },
        ObjectFormat::Sha256,
        OidPair {
            base: oid('a'),
            candidate: oid('b'),
        },
        OidPair {
            base: oid('c'),
            candidate: oid('d'),
        },
    );

    assert!(invalid.is_none());
}
