use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller::{
    AcquiringRunner, ArtifactStoreConfig, ChangeState, CheckConclusion, CheckPlan, ControllerClock,
    DeliveryRoute, FileArtifactStore, FileLedger, FileLedgerConfig, IngressLimits, IngressPolicy,
    OpaqueId, PlanRegistry, PlanScope, PolicyControls, ProviderAdapter, ProviderIdentity,
    ProviderInstance, ProviderNamespace, ReplayWindow, SignedTimePolicy, WebhookKey,
    WebhookKeyring, check_plan, register_plan,
};
use amiss_controller_fixtures::clock::TestClock;
use amiss_controller_github::{GitHubPullRequestAdapter, GitHubPullRequestSource};
use amiss_controller_service::{
    AcquiringWorkerContext, AcquiringWorkerSettings, AdmissionRejection, AdmissionRequest,
    AdmittedDelivery, DeliveryAdmission, DeliveryHeader, DeliveryWorker, Inbox, InboxLimits,
    IncomingDelivery, IncomingHeader, Operations, WorkOutcome, acquiring_worker,
    repository_admission,
};
use amiss_wire::controls::Profile;
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use tempfile::TempDir;

use super::provider::{CHECK_RUN_BODY, FakeGitHub, REPOSITORY_ID, SignedEvent, snapshot};
use amiss_controller_fixtures::lane::{CopyAcquisition, Repositories, execution_constraint};

const SECRET: &[u8] = b"provider-lane-webhook-secret-2026";
const ROUTE_ID: &str = "github-provider-lane";

#[derive(Clone, Copy)]
pub(super) enum LaneCase {
    Pass,
    WrongIdentity,
    WrongTree,
    Revoked,
    MissingOutput,
    Timeout,
    TamperedRuntime,
}

pub(super) struct Harness {
    pub(super) clock: Arc<TestClock>,
    _state: TempDir,
    repositories: Repositories,
    event: SignedEvent,
    admission: Arc<dyn DeliveryAdmission>,
    pub inbox: Arc<Mutex<Inbox>>,
    pub worker: DeliveryWorker<FileLedger, AcquiringRunner<CopyAcquisition>>,
    pub api: FakeGitHub,
}

struct ProviderSetup {
    route: DeliveryRoute,
    event: SignedEvent,
    admission: Arc<dyn DeliveryAdmission>,
    api: FakeGitHub,
    adapter: Arc<dyn ProviderAdapter>,
    plans: PlanRegistry,
}

impl Harness {
    pub(super) fn new(case: LaneCase, queue_age: Duration) -> Self {
        let state = TempDir::new().unwrap();
        let scratch = directory(&state, "scratch");
        let inbox_root = directory(&state, "inbox");
        let ledger_root = directory(&state, "ledger");
        let artifact_root = directory(&state, "artifacts");
        let repositories = Repositories::new().unwrap();
        let executable =
            PathBuf::from(env!("CARGO_BIN_EXE_amiss-github-service-bootstrap-fixture"));
        let bootstrap_digest = hb(BOOTSTRAP_DOMAIN, &std::fs::read(&executable).unwrap());
        let execution = execution_constraint(
            &repositories,
            RepositoryIdentity::github("hardmax71".to_owned(), "amiss".to_owned()).unwrap(),
            case.status(),
            bootstrap_digest,
        )
        .unwrap();
        let plan =
            Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution).unwrap());
        let replay = ReplayWindow::new(Duration::from_mins(5), queue_age).unwrap();
        let ingress = IngressPolicy::new(
            IngressLimits::new(1_000_000, 32, 8_192).unwrap(),
            replay,
            Duration::from_secs(5),
        )
        .unwrap();
        let test_clock = TestClock::new();
        let clock: Arc<dyn ControllerClock> = test_clock.clone();
        let ProviderSetup {
            route,
            event,
            admission,
            api,
            adapter,
            plans,
        } = provider_setup(case, &repositories, ingress, plan, Arc::clone(&clock));
        let ledger = FileLedger::open_with_clock(
            &ledger_root,
            FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
            Arc::clone(&clock),
        )
        .unwrap();
        let artifacts = Arc::new(
            FileArtifactStore::open_with_clock(
                &artifact_root,
                ArtifactStoreConfig {
                    base_url: "https://amiss.example/artifacts".to_owned(),
                    retention: Duration::from_hours(1),
                    max_records: 32,
                    max_bytes: 16 * 1_024 * 1_024,
                    max_record_bytes: 16 * 1_024 * 1_024,
                },
                Arc::clone(&clock),
            )
            .unwrap(),
        );
        let inbox = Arc::new(Mutex::new(
            Inbox::open(&inbox_root, inbox_limits()).unwrap(),
        ));
        let worker = acquiring_worker(
            AcquiringWorkerContext {
                settings: AcquiringWorkerSettings {
                    bootstrap: executable_for(case, &state, &executable),
                    scratch,
                    bootstrap_timeout: case.wall_timeout(),
                    statement_validity: Duration::from_mins(5),
                    ingress,
                    route,
                    route_id: ROUTE_ID.to_owned(),
                    retry_min: Duration::from_millis(50),
                    retry_max: Duration::from_millis(100),
                    idle_poll: Duration::from_millis(5),
                },
                plans,
                ledger,
                admission: Arc::clone(&admission),
                clock,
                artifacts,
            },
            Arc::clone(&inbox),
            Operations::default(),
            adapter,
            repositories.acquisition(),
        )
        .unwrap();
        Self {
            clock: Arc::clone(&test_clock),
            _state: state,
            repositories,
            event,
            admission,
            inbox,
            worker,
            api,
        }
    }

    pub(super) fn enqueue(&self) {
        let received_at_unix_millis = self.clock.now();
        let headers = [DeliveryHeader {
            name: "x-hub-signature-256".to_owned(),
            value: self.event.signature.clone(),
        }];
        let admitted = self
            .admission
            .admit(AdmissionRequest {
                received_at_unix_millis,
                headers: &headers,
                body: &self.event.body,
            })
            .unwrap()
            .unwrap();
        let stored_headers = [IncomingHeader {
            name: "x-hub-signature-256",
            value: &self.event.signature,
        }];
        self.inbox
            .lock()
            .unwrap()
            .enqueue(IncomingDelivery {
                route: &admitted.route,
                source_id: &admitted.source_id,
                received_at_unix_millis,
                headers: &stored_headers,
                body: &self.event.body,
            })
            .unwrap();
    }

    pub(super) fn target_rejection(&self, target: &str) -> Option<AdmissionRejection> {
        let event = SignedEvent::for_target(&self.repositories.commits.candidate, target, SECRET);
        let headers = [DeliveryHeader {
            name: "x-hub-signature-256".to_owned(),
            value: event.signature,
        }];
        self.admission
            .admit(AdmissionRequest {
                received_at_unix_millis: event.received_at_unix_millis,
                headers: &headers,
                body: &event.body,
            })
            .err()
    }

    pub(super) fn no_work(&self) -> Result<Option<AdmittedDelivery>, AdmissionRejection> {
        let event = SignedEvent::signed(CHECK_RUN_BODY.to_vec(), SECRET);
        let headers = [DeliveryHeader {
            name: "x-hub-signature-256".to_owned(),
            value: event.signature,
        }];
        self.admission.admit(AdmissionRequest {
            received_at_unix_millis: event.received_at_unix_millis,
            headers: &headers,
            body: &event.body,
        })
    }

    pub(super) fn work(&mut self) -> WorkOutcome {
        self.worker.work_once().unwrap()
    }

    pub(super) fn expect_conclusion(&self, expected: Option<CheckConclusion>) {
        let last = self
            .api
            .publications()
            .last()
            .map(|publication| publication.conclusion);
        assert!(
            last == expected,
            "expected {expected:?}, got {last:?}; {}",
            self.api.flow_trace()
        );
    }
}

fn provider_setup(
    case: LaneCase,
    repositories: &Repositories,
    ingress: IngressPolicy,
    plan: Arc<CheckPlan>,
    clock: Arc<dyn ControllerClock>,
) -> ProviderSetup {
    let provider = provider();
    let route = route(&provider);
    let source = Arc::new(GitHubPullRequestSource::new(
        provider.clone(),
        webhook(),
        &plan.policy.workflow_artifacts,
    ));
    let event = SignedEvent::new(&repositories.commits.candidate, SECRET);
    let delivery = event.delivery(&route, ingress, &source);
    let mut current = snapshot(
        &delivery,
        case.state(),
        repositories.commits.clone(),
        repositories.trees.clone(),
    );
    if matches!(case, LaneCase::WrongIdentity) {
        current.run.change.repository = RepositoryIdentity::new(
            current.run.change.repository.host().to_owned(),
            current.run.change.repository.owner().to_owned(),
            "another".to_owned(),
        )
        .unwrap();
    }
    if matches!(case, LaneCase::WrongTree) {
        current.run.trees.candidate = oid('f');
    }
    let api = FakeGitHub::new([Ok(current.clone()), Ok(current)]);
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(GitHubPullRequestAdapter::from_source(
        Arc::clone(&source),
        api.clone(),
    ));
    let mut plans = PlanRegistry::new();
    register_plan(
        &mut plans,
        PlanScope {
            provider,
            integration: delivery.identity.integration.clone(),
            repository: delivery.change.repository.clone(),
        },
        plan,
    )
    .unwrap();
    let target = BranchRef::new("refs/heads/main".to_owned()).unwrap();
    let admission = repository_admission(
        ROUTE_ID.to_owned(),
        route.clone(),
        ingress,
        plans.clone(),
        clock,
        REPOSITORY_ID,
        move |checked| source.authenticate_for_target(checked, &target),
    );
    ProviderSetup {
        route,
        event,
        admission,
        api,
        adapter,
        plans,
    }
}

impl LaneCase {
    fn status(self) -> &'static str {
        match self {
            Self::MissingOutput => "runner-missing",
            Self::Timeout => "runner-hang",
            Self::Pass
            | Self::WrongIdentity
            | Self::WrongTree
            | Self::Revoked
            | Self::TamperedRuntime => "runner-pass",
        }
    }

    const fn state(self) -> ChangeState {
        if matches!(self, Self::Revoked) {
            ChangeState::AuthorizationRevoked
        } else {
            ChangeState::Active
        }
    }

    const fn wall_timeout(self) -> Duration {
        if matches!(self, Self::Timeout) {
            Duration::from_millis(50)
        } else {
            Duration::from_secs(10)
        }
    }
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.com".to_owned()).unwrap(),
    }
}

fn route(provider: &ProviderIdentity) -> DeliveryRoute {
    DeliveryRoute {
        provider: provider.clone(),
        trust_set: OpaqueId::new("github-provider-lane-keys".to_owned()).unwrap(),
        signed_time: SignedTimePolicy::ReplayOnly,
    }
}

fn webhook() -> amiss_controller::GitHubWebhook {
    let key = WebhookKey::new(
        OpaqueId::new("current".to_owned()).unwrap(),
        SECRET.to_vec(),
        0,
        None,
    )
    .unwrap();
    amiss_controller::GitHubWebhook::new(
        WebhookKeyring::new(
            OpaqueId::new("github-provider-lane-keys".to_owned()).unwrap(),
            vec![key],
        )
        .unwrap(),
    )
}

fn executable_for(case: LaneCase, state: &TempDir, executable: &std::path::Path) -> PathBuf {
    if !matches!(case, LaneCase::TamperedRuntime) {
        return executable.to_path_buf();
    }
    let changed = state.path().join("changed-bootstrap");
    std::fs::write(&changed, b"changed after the plan was fixed").unwrap();
    changed
}

fn directory(root: &TempDir, name: &str) -> PathBuf {
    let path = root.path().join(name);
    std::fs::create_dir(&path).unwrap();
    path
}

fn inbox_limits() -> InboxLimits {
    InboxLimits {
        lease_duration: Duration::from_secs(2),
        max_records: 16,
        max_bytes: 16_777_216,
        max_record_bytes: 2_097_152,
        max_body_bytes: 1_000_000,
        max_headers: 32,
        max_header_bytes: 8_192,
        max_route_bytes: 128,
        max_source_id_bytes: 128,
    }
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}
