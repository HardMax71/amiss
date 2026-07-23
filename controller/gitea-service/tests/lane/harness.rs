use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller::{
    AcquiringRunner, AdapterRegistry, CheckConclusion, CheckPlan, Controller, ControllerClock,
    DeliveryRoute, FileLedger, FileLedgerConfig, GiteaWebhook, IngressLimits, IngressPolicy,
    OpaqueId, PlanRegistry, PlanScope, PolicyControls, ProviderAdapter, ProviderError,
    ReplayWindow, SignedTimePolicy, SystemClock, WebhookKey, WebhookKeyring, check_plan,
    register_plan,
};
use amiss_controller_gitea::{GiteaPullRequestAdapter, GiteaPullRequestSource};
use amiss_controller_service::{
    AdmissionRejection, AdmissionRequest, DeliveryAdmission, DeliveryHeader, DeliveryWorker,
    DeliveryWorkerInput, Inbox, InboxLimits, IncomingDelivery, IncomingHeader, WorkOutcome,
    lane_admission,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use tempfile::TempDir;

use super::provider::{
    FakeGitea, REPOSITORY_ID, SignedEvent, last_conclusion, provider, reviewer, snapshot,
};
use super::repositories::{CopyAcquisition, Repositories};

const SECRET: &[u8] = b"gitea-family-provider-lane-secret-2026";
const ROUTE_ID: &str = "gitea-family-provider-lane";

#[derive(Clone, Copy)]
pub(super) struct LaneSettings {
    pub namespace: &'static str,
    pub signature_header: &'static str,
    pub provider_reviewer_id: u64,
    pub wrong_tree: bool,
    pub tampered_runtime: bool,
    pub publish_failures: usize,
    pub refresh_failure: Option<ProviderError>,
}

impl LaneSettings {
    pub(super) const fn pass(namespace: &'static str, signature_header: &'static str) -> Self {
        Self {
            namespace,
            signature_header,
            provider_reviewer_id: super::provider::REVIEWER_ID,
            wrong_tree: false,
            tampered_runtime: false,
            publish_failures: 0,
            refresh_failure: None,
        }
    }
}

pub(super) struct Harness {
    _state: TempDir,
    repositories: Repositories,
    signature_header: &'static str,
    event: SignedEvent,
    admission: Arc<dyn DeliveryAdmission>,
    pub inbox: Arc<Mutex<Inbox>>,
    pub worker: DeliveryWorker<FileLedger, AcquiringRunner<CopyAcquisition>>,
    pub api: FakeGitea,
}

struct ProviderSetup {
    route: DeliveryRoute,
    event: SignedEvent,
    admission: Arc<dyn DeliveryAdmission>,
    api: FakeGitea,
    adapters: AdapterRegistry,
    plans: PlanRegistry,
}

impl Harness {
    pub(super) fn new(settings: LaneSettings, queue_age: Duration) -> Self {
        let state = TempDir::new().unwrap();
        let scratch = directory(&state, "scratch");
        let inbox_root = directory(&state, "inbox");
        let ledger_root = directory(&state, "ledger");
        let repositories = Repositories::new();
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_amiss-gitea-service-bootstrap-fixture"));
        let bootstrap_digest = hb(BOOTSTRAP_DOMAIN, &std::fs::read(&executable).unwrap());
        let plan = Arc::new(
            check_plan(
                Profile::Enforce,
                PolicyControls::default(),
                execution(&repositories, bootstrap_digest),
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
        } = provider_setup(settings, &repositories, ingress, plan);
        let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
        let ledger = FileLedger::open_with_clock(
            &ledger_root,
            FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
            Arc::clone(&clock),
        )
        .unwrap();
        let runner = AcquiringRunner::new(
            repositories.acquisition(),
            executable_for(settings.tampered_runtime, &state, &executable),
            scratch,
            Duration::from_secs(1),
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
        let worker = DeliveryWorker::new(DeliveryWorkerInput {
            inbox: Arc::clone(&inbox),
            controller,
            admission: Arc::clone(&admission),
            route,
            route_id: ROUTE_ID.to_owned(),
            retry_min: Duration::from_millis(20),
            retry_max: Duration::from_millis(100),
            idle_poll: Duration::from_millis(5),
            clock,
        })
        .unwrap();
        Self {
            _state: state,
            repositories,
            signature_header: settings.signature_header,
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
            name: self.signature_header.to_owned(),
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
            name: self.signature_header,
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
            name: self.signature_header.to_owned(),
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

    pub(super) fn conclusion(&self) -> Option<CheckConclusion> {
        last_conclusion(&self.api)
    }
}

fn provider_setup(
    settings: LaneSettings,
    repositories: &Repositories,
    ingress: IngressPolicy,
    plan: Arc<CheckPlan>,
) -> ProviderSetup {
    let provider = provider(settings.namespace);
    let route = route(&provider);
    let source =
        Arc::new(GiteaPullRequestSource::new(provider.clone(), reviewer(), webhook()).unwrap());
    let event = SignedEvent::new(&repositories.commits().candidate, SECRET);
    let delivery = event.delivery(&route, ingress, &source, settings.signature_header);
    let mut current = snapshot(&delivery, repositories.commits(), repositories.trees());
    if settings.wrong_tree {
        current.run.trees.candidate = oid('f');
    }
    let refreshes = settings.refresh_failure.map_or_else(
        || vec![Ok(current.clone()), Ok(current)],
        |error| vec![Err(error)],
    );
    let api = FakeGitea::new(
        settings.provider_reviewer_id,
        refreshes,
        settings.publish_failures,
    );
    let adapter = Arc::new(GiteaPullRequestAdapter::from_source(
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
                .map_err(|error| {
                    if error == ProviderError::AuthorizationRevoked {
                        AdmissionRejection::Forbidden
                    } else {
                        AdmissionRejection::Unauthorized
                    }
                })?;
            verified
                .delivery()
                .change
                .change
                .as_str()
                .starts_with(&format!("repository/{REPOSITORY_ID}/"))
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

fn execution(
    repositories: &Repositories,
    bootstrap_digest: amiss_wire::digest::Digest,
) -> ExecutionConstraintDescriptor {
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let mut input = ExecutionConstraintInput::from(&template);
    input.action_repository = RepositoryIdentity::new(
        "forge.example".to_owned(),
        "hardmax71".to_owned(),
        "amiss".to_owned(),
    )
    .unwrap();
    input.action_object_format = ObjectFormat::Sha1;
    input.action_commit_oid = repositories.action_commit();
    input.action_tree_oid = repositories.action_tree();
    "runner-pass".clone_into(&mut input.required_status_name);
    input.bootstrap_digest = bootstrap_digest;
    ExecutionConstraintDescriptor::new(input).unwrap()
}

fn route(provider: &amiss_controller::ProviderIdentity) -> DeliveryRoute {
    DeliveryRoute {
        provider: provider.clone(),
        trust_set: OpaqueId::new("gitea-family-provider-lane-keys".to_owned()).unwrap(),
        signed_time: SignedTimePolicy::ReplayOnly,
    }
}

fn webhook() -> GiteaWebhook {
    let key = WebhookKey::new(
        OpaqueId::new("current".to_owned()).unwrap(),
        SECRET.to_vec(),
        0,
        None,
    )
    .unwrap();
    GiteaWebhook::new(
        WebhookKeyring::new(
            OpaqueId::new("gitea-family-provider-lane-keys".to_owned()).unwrap(),
            vec![key],
        )
        .unwrap(),
    )
}

fn executable_for(tampered: bool, state: &TempDir, executable: &std::path::Path) -> PathBuf {
    if !tampered {
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
        max_record_bytes: 1_048_576,
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
