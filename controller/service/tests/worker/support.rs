use std::collections::VecDeque;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use amiss_controller::{
    AdapterRegistry, AuthenticatedDelivery, ChangeId, ChangeLocator, ChangeSnapshot, ChangeState,
    Controller, ControllerClock, DeliveryId, DeliveryIdentity, DeliveryRoute, Evaluation,
    FileLedger, FileLedgerConfig, GitHubWebhook, HeartbeatOutcome, IngressCheck, IngressLimits,
    IngressPolicy, IntegrationId, OidPair, OpaqueId, PlanRegistry, PlanScope, PolicyControls,
    ProviderAdapter, ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, Publication, ReplayWindow,
    RunHeartbeat, RunIdentity, RunRefs, RunRequest, Runner, RunnerOutcome, SignedTimePolicy,
    SystemClock, VerifiedDelivery, WebhookKey, WebhookKeyring, check_plan, register_plan,
};
use amiss_controller_service::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission, DeliveryHeader,
    DeliveryWorker, DeliveryWorkerInput, Inbox, InboxLimits, IncomingDelivery, IncomingHeader,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;
use tempfile::TempDir;

const BODY: &[u8] = br#"{"event":"change"}"#;
const ROUTE_ID: &str = "github-main";
const SOURCE_ID: &str = "source-1";
const SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";
const STEADY_LEASE: Duration = Duration::from_secs(30);
const RENEWAL_LEASE: Duration = Duration::from_secs(2);

pub(crate) enum Refresh {
    Active,
    Error(ProviderError),
}

pub(crate) struct Admission {
    pub(crate) accept: AtomicBool,
    pub(crate) calls: AtomicUsize,
}

impl Admission {
    const fn new() -> Self {
        Self {
            accept: AtomicBool::new(true),
            calls: AtomicUsize::new(0),
        }
    }
}

impl DeliveryAdmission for Admission {
    fn admit(&self, request: AdmissionRequest<'_>) -> Result<AdmittedDelivery, AdmissionRejection> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if !self.accept.load(Ordering::Acquire)
            || request.body != BODY
            || !request
                .headers
                .iter()
                .any(|header| valid_signature(header, request.body))
        {
            return Err(AdmissionRejection::Unauthorized);
        }
        Ok(AdmittedDelivery {
            route: ROUTE_ID.to_owned(),
            source_id: SOURCE_ID.to_owned(),
        })
    }
}

fn valid_signature(header: &DeliveryHeader, body: &[u8]) -> bool {
    let Some(encoded) = header
        .value
        .strip_prefix(b"sha256=")
        .filter(|_value| header.name == "x-hub-signature-256")
    else {
        return false;
    };
    let Ok(signature) = hex::decode(encoded) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(SECRET) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}

pub(crate) struct Adapter {
    namespace: ProviderNamespace,
    verifier: GitHubWebhook,
    authenticated: AuthenticatedDelivery,
    run: RunIdentity,
    refreshes: Mutex<VecDeque<Refresh>>,
    pub(crate) authentications: AtomicUsize,
    pub(crate) publications: AtomicUsize,
}

impl ProviderAdapter for Adapter {
    fn namespace(&self) -> &ProviderNamespace {
        &self.namespace
    }

    fn authenticate(&self, delivery: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        self.authentications.fetch_add(1, Ordering::Relaxed);
        self.verifier
            .verify(delivery)
            .map(|proof| proof.bind(self.authenticated.clone()))
            .map_err(|_error| ProviderError::Authentication)
    }

    fn refresh(&self, _delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        match self
            .refreshes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Refresh::Error(ProviderError::Unavailable))
        {
            Refresh::Active => Ok(ChangeSnapshot {
                state: ChangeState::Active,
                run: self.run.clone(),
                gate_commit: self.run.commits.candidate.clone(),
            }),
            Refresh::Error(error) => Err(error),
        }
    }

    fn publish(
        &self,
        _delivery: &AuthenticatedDelivery,
        _publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.publications.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

pub(crate) struct TestRunner {
    run: RunIdentity,
    delay: Duration,
    release: Option<Arc<AtomicBool>>,
    started: Arc<Barrier>,
}

impl Runner for TestRunner {
    fn run(&mut self, _request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome {
        self.started.wait();
        let deadline = Instant::now() + self.delay;
        loop {
            if heartbeat.renew() == HeartbeatOutcome::Stop {
                return RunnerOutcome::Unavailable;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let wait = match &self.release {
                Some(release) if release.load(Ordering::Acquire) => break,
                Some(_release) => Duration::from_millis(20),
                None if remaining.is_zero() => break,
                None => remaining.min(Duration::from_millis(20)),
            };
            std::thread::sleep(wait);
        }
        RunnerOutcome::Complete {
            identity: Box::new(self.run.clone()),
            evaluation: Evaluation::Pass,
            report: br#"{"schema":"amiss/report"}"#.to_vec(),
        }
    }
}

pub(crate) struct Fixture {
    _temporary: TempDir,
    pub(crate) worker: DeliveryWorker<FileLedger, TestRunner>,
    pub(crate) inbox: Arc<Mutex<Inbox>>,
    pub(crate) admission: Arc<Admission>,
    pub(crate) adapter: Arc<Adapter>,
    pub(crate) run_started: Arc<Barrier>,
}

impl Fixture {
    pub(crate) fn new(
        refreshes: impl IntoIterator<Item = Refresh>,
        runner_delay: Duration,
    ) -> Self {
        Self::build(refreshes, runner_delay, None, STEADY_LEASE)
    }

    pub(crate) fn held(refreshes: impl IntoIterator<Item = Refresh>) -> (Self, Arc<AtomicBool>) {
        let release = Arc::new(AtomicBool::new(false));
        (
            Self::build(
                refreshes,
                Duration::ZERO,
                Some(Arc::clone(&release)),
                RENEWAL_LEASE,
            ),
            release,
        )
    }

    fn build(
        refreshes: impl IntoIterator<Item = Refresh>,
        runner_delay: Duration,
        release: Option<Arc<AtomicBool>>,
        inbox_lease: Duration,
    ) -> Self {
        let temporary = TempDir::new().unwrap();
        let inbox_root = temporary.path().join("inbox");
        let ledger_root = temporary.path().join("ledger");
        fs::create_dir(&inbox_root).unwrap();
        fs::create_dir(&ledger_root).unwrap();
        let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
        let run = run();
        let authenticated = authenticated();
        let adapter = Arc::new(Adapter {
            namespace: authenticated.identity.provider.namespace.clone(),
            verifier: verifier(),
            authenticated: authenticated.clone(),
            run: run.clone(),
            refreshes: Mutex::new(refreshes.into_iter().collect()),
            authentications: AtomicUsize::new(0),
            publications: AtomicUsize::new(0),
        });
        let mut registry = AdapterRegistry::new();
        let registered: Arc<dyn ProviderAdapter> = adapter.clone();
        registry.register(registered).unwrap();
        let plan = Arc::new(plan());
        let mut plans = PlanRegistry::new();
        register_plan(
            &mut plans,
            PlanScope {
                provider: authenticated.identity.provider.clone(),
                integration: authenticated.identity.integration.clone(),
                repository: authenticated.change.repository.clone(),
            },
            plan,
        )
        .unwrap();
        let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_secs(30)).unwrap();
        let ledger_config = FileLedgerConfig::new(STEADY_LEASE, 16, replay).unwrap();
        let ledger =
            FileLedger::open_with_clock(&ledger_root, ledger_config, Arc::clone(&clock)).unwrap();
        let started = Arc::new(Barrier::new(
            if runner_delay.is_zero() && release.is_none() {
                1
            } else {
                2
            },
        ));
        let runner = TestRunner {
            run,
            delay: runner_delay,
            release,
            started: Arc::clone(&started),
        };
        let ingress = IngressPolicy::new(
            IngressLimits::new(4_096, 8, 2_048).unwrap(),
            replay,
            Duration::from_secs(5),
        )
        .unwrap();
        let controller = Controller::new_with_clock(
            registry,
            plans,
            ledger,
            runner,
            ingress,
            Arc::clone(&clock),
        );
        let inbox = Arc::new(Mutex::new(
            Inbox::open(&inbox_root, inbox_limits(inbox_lease)).unwrap(),
        ));
        let admission = Arc::new(Admission::new());
        let shared_admission: Arc<dyn DeliveryAdmission> = admission.clone();
        let worker = DeliveryWorker::new(DeliveryWorkerInput {
            inbox: Arc::clone(&inbox),
            controller,
            admission: shared_admission,
            route: route(),
            route_id: ROUTE_ID.to_owned(),
            retry_min: Duration::from_millis(120),
            retry_max: Duration::from_secs(1),
            idle_poll: Duration::from_millis(5),
            clock,
        })
        .unwrap();
        Self {
            _temporary: temporary,
            worker,
            inbox,
            admission,
            adapter,
            run_started: started,
        }
    }
}

pub(crate) fn enqueue(inbox: &Arc<Mutex<Inbox>>, admission: &Arc<Admission>) {
    let received_at = now();
    let signature = signature(BODY);
    let headers = [DeliveryHeader {
        name: "x-hub-signature-256".to_owned(),
        value: signature.clone(),
    }];
    let admitted = admission
        .admit(AdmissionRequest {
            received_at_unix_millis: received_at,
            headers: &headers,
            body: BODY,
        })
        .unwrap();
    let incoming_headers = [IncomingHeader {
        name: "x-hub-signature-256",
        value: &signature,
    }];
    inbox
        .lock()
        .unwrap()
        .enqueue(IncomingDelivery {
            route: &admitted.route,
            source_id: &admitted.source_id,
            received_at_unix_millis: received_at,
            headers: &incoming_headers,
            body: BODY,
        })
        .unwrap();
}

pub(crate) fn now() -> i64 {
    SystemClock.now_unix_millis().unwrap()
}

fn signature(body: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes())).into_bytes()
}

fn verifier() -> GitHubWebhook {
    let trust_set = OpaqueId::new("webhooks-main".to_owned()).unwrap();
    let anchor = OpaqueId::new("anchor-current".to_owned()).unwrap();
    let key = WebhookKey::new(anchor, SECRET.to_vec(), 0, None).unwrap();
    GitHubWebhook::new(WebhookKeyring::new(trust_set, vec![key]).unwrap())
}

fn route() -> DeliveryRoute {
    DeliveryRoute {
        provider: provider(),
        trust_set: OpaqueId::new("webhooks-main".to_owned()).unwrap(),
        signed_time: SignedTimePolicy::ReplayOnly,
    }
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.com".to_owned()).unwrap(),
    }
}

fn authenticated() -> AuthenticatedDelivery {
    let provider = provider();
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("installation-7".to_owned()).unwrap(),
            delivery: DeliveryId::new("placeholder".to_owned()).unwrap(),
        },
        change: change(provider),
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("provider-run-11".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            oid('b'),
        )
        .unwrap(),
    }
}

fn change(provider: ProviderIdentity) -> ChangeLocator {
    ChangeLocator {
        provider,
        repository: RepositoryIdentity::new(
            "github.com".to_owned(),
            "owner".to_owned(),
            "amiss".to_owned(),
        )
        .unwrap(),
        change: ChangeId::new("42".to_owned()).unwrap(),
    }
}

fn run() -> RunIdentity {
    RunIdentity::new(
        change(provider()),
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
    .unwrap()
}

fn oid(byte: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, byte.to_string().repeat(40)).unwrap()
}

fn plan() -> amiss_controller::CheckPlan {
    let execution = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    check_plan(Profile::Enforce, PolicyControls::default(), execution).unwrap()
}

fn inbox_limits(lease_duration: Duration) -> InboxLimits {
    InboxLimits {
        lease_duration,
        max_records: 8,
        max_bytes: 262_144,
        max_record_bytes: 131_072,
        max_body_bytes: 4_096,
        max_headers: 8,
        max_header_bytes: 2_048,
        max_route_bytes: 128,
        max_source_id_bytes: 128,
    }
}
