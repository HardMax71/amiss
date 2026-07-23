use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller::{
    AcquiringRunner, AdapterRegistry, ChangeState, CheckConclusion, CheckPlan, Controller,
    ControllerClock, DeliveryRoute, FileLedger, FileLedgerConfig, IngressLimits, IngressPolicy,
    OpaqueId, PlanRegistry, PlanScope, PolicyControls, ProviderAdapter, ProviderError,
    ProviderIdentity, ProviderInstance, ProviderNamespace, ReplayWindow, SignedTimePolicy,
    SystemClock, WebhookKey, WebhookKeyring, check_plan, register_plan,
};
use amiss_controller_github::{GitHubPullRequestAdapter, GitHubPullRequestSource};
use amiss_controller_service::{
    AdmissionRejection, AdmissionRequest, DeliveryAdmission, DeliveryHeader, DeliveryWorker,
    DeliveryWorkerInput, Inbox, InboxLimits, IncomingDelivery, IncomingHeader, WorkOutcome,
    lane_admission,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use tempfile::TempDir;

use super::provider::{FakeGitHub, SignedEvent, snapshot};
use super::repositories::{CopyAcquisition, Repositories};

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
    adapters: AdapterRegistry,
    plans: PlanRegistry,
}

impl Harness {
    pub(super) fn new(case: LaneCase, queue_age: Duration) -> Self {
        let state = TempDir::new().unwrap();
        let scratch = directory(&state, "scratch");
        let inbox_root = directory(&state, "inbox");
        let ledger_root = directory(&state, "ledger");
        let repositories = Repositories::new();
        let executable =
            PathBuf::from(env!("CARGO_BIN_EXE_amiss-github-service-bootstrap-fixture"));
        let bootstrap_digest = hb(BOOTSTRAP_DOMAIN, &std::fs::read(&executable).unwrap());
        let plan = Arc::new(
            check_plan(
                Profile::Enforce,
                PolicyControls::default(),
                execution(&repositories, case.status(), bootstrap_digest),
            )
            .unwrap(),
        );
        let replay = ReplayWindow::new(Duration::from_mins(5), queue_age).unwrap();
        let ingress = IngressPolicy::new(
            IngressLimits::new(1_000_000, 32, 8_192).unwrap(),
            replay,
            Duration::from_secs(5),
        )
        .unwrap();
        let ProviderSetup {
            route,
            event,
            admission,
            api,
            adapters,
            plans,
        } = provider_setup(case, &repositories, ingress, plan);
        let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
        let ledger = FileLedger::open_with_clock(
            &ledger_root,
            FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
            Arc::clone(&clock),
        )
        .unwrap();
        let runner = AcquiringRunner::new(
            repositories.acquisition(),
            executable_for(case, &state, &executable),
            scratch,
            case.wall_timeout(),
            Duration::from_mins(5),
            Arc::clone(&clock),
        )
        .unwrap();
        let controller = Controller::new_with_clock(
            adapters,
            plans,
            ledger,
            runner,
            ingress,
            Arc::clone(&clock),
        );
        let inbox = Arc::new(Mutex::new(
            Inbox::open(&inbox_root, inbox_limits()).unwrap(),
        ));
        let shared_admission: Arc<dyn DeliveryAdmission> = admission.clone();
        let worker = DeliveryWorker::new(DeliveryWorkerInput {
            inbox: Arc::clone(&inbox),
            controller,
            admission: shared_admission,
            route,
            route_id: ROUTE_ID.to_owned(),
            retry_min: Duration::from_millis(50),
            retry_max: Duration::from_millis(100),
            idle_poll: Duration::from_millis(5),
            clock,
        })
        .unwrap();
        Self {
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
        let received_at_unix_millis = SystemClock.now_unix_millis().unwrap();
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
        let event = SignedEvent::for_target(&self.repositories.commits().candidate, target, SECRET);
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
) -> ProviderSetup {
    let provider = provider();
    let route = route(&provider);
    let source = Arc::new(GitHubPullRequestSource::new(provider.clone(), webhook()));
    let event = SignedEvent::new(&repositories.commits().candidate, SECRET);
    let delivery = event.delivery(&route, ingress, &source);
    let mut current = snapshot(
        &delivery,
        case.state(),
        repositories.commits(),
        repositories.trees(),
    );
    if matches!(case, LaneCase::WrongIdentity) {
        "another".clone_into(&mut current.run.change.repository.name);
    }
    if matches!(case, LaneCase::WrongTree) {
        current.run.trees.candidate = oid('f');
    }
    let api = FakeGitHub::new([Ok(current.clone()), Ok(current)]);
    let adapter = Arc::new(GitHubPullRequestAdapter::from_source(
        Arc::clone(&source),
        api.clone(),
    ));
    let mut adapters = AdapterRegistry::new();
    let registered: Arc<dyn ProviderAdapter> = adapter;
    adapters.register(registered).unwrap();
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
    let admission: Arc<dyn DeliveryAdmission> = Arc::new(lane_admission(
        ROUTE_ID.to_owned(),
        route.clone(),
        ingress,
        plans.clone(),
        move |checked| {
            let verified = source
                .authenticate_for_target(checked, &target)
                .map_err(|error| match error {
                    ProviderError::AuthorizationRevoked => AdmissionRejection::Forbidden,
                    ProviderError::Authentication
                    | ProviderError::Unavailable
                    | ProviderError::InvalidResponse => AdmissionRejection::Unauthorized,
                })?;
            verified
                .delivery()
                .change
                .change
                .as_str()
                .starts_with("repository/101/")
                .then_some(verified)
                .ok_or(AdmissionRejection::Forbidden)
        },
    ));
    ProviderSetup {
        route,
        event,
        admission,
        api,
        adapters,
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

fn execution(
    repositories: &Repositories,
    status: &str,
    bootstrap_digest: amiss_wire::digest::Digest,
) -> ExecutionConstraintDescriptor {
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let mut input = ExecutionConstraintInput::from(&template);
    input.action_repository =
        RepositoryIdentity::github("hardmax71".to_owned(), "amiss".to_owned()).unwrap();
    input.action_object_format = ObjectFormat::Sha1;
    input.action_commit_oid = repositories.action_commit();
    input.action_tree_oid = repositories.action_tree();
    status.clone_into(&mut input.required_status_name);
    input.bootstrap_digest = bootstrap_digest;
    ExecutionConstraintDescriptor::new(input).unwrap()
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
