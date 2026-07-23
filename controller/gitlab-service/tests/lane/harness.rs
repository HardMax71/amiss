use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller::{
    AcquiringRunner, AdapterRegistry, Controller, ControllerClock, DeliveryHeader, DeliveryRoute,
    FileLedger, FileLedgerConfig, IngressLimits, IngressPolicy, OpaqueId, PlanRegistry, PlanScope,
    PolicyControls, ProviderAdapter, ReplayWindow, SignedTimePolicy, SystemClock,
    UntrustedDelivery, check_plan, register_plan,
};
use amiss_controller_gitlab::{GitLabMergeTrainAdapter, policy_job_accepted};
use amiss_controller_service::{
    AdmissionRejection, EvaluationConfig, check_lane, evaluation_router,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt as _;

use super::provider::{FakeGitLab, HOST, claims, policy, provider, refresh, sign, source};
use super::repositories::{CopyAcquisition, Repositories};

const ENDPOINT: &str = "/gitlab/policy/evaluate";

#[derive(Clone, Copy)]
pub(super) enum LaneCase {
    Pass,
    Block,
    MissingOutput,
    Timeout,
    WrongTree,
    WrongParents,
    TamperedBootstrap,
    FinalPolicyRevoked,
    FinalGateChanged,
}

pub(super) struct Harness {
    _state: TempDir,
    repositories: Repositories,
    router: Router,
    token: String,
    pub(super) api: FakeGitLab,
}

struct Lane {
    route: DeliveryRoute,
    adapter: Arc<dyn ProviderAdapter>,
    plans: PlanRegistry,
    ledger: FileLedgerConfig,
    ledger_root: PathBuf,
    ingress: IngressPolicy,
    acquisition: CopyAcquisition,
    executable: PathBuf,
    scratch: PathBuf,
    wall_timeout: Duration,
}

impl Harness {
    pub(super) fn new(case: LaneCase) -> Self {
        let state = TempDir::new().unwrap();
        let scratch = directory(&state, "scratch");
        let ledger_root = directory(&state, "ledger");
        let repositories = Repositories::new();
        let executable =
            PathBuf::from(env!("CARGO_BIN_EXE_amiss-gitlab-service-bootstrap-fixture"));
        let bootstrap_digest = hb(BOOTSTRAP_DOMAIN, &std::fs::read(&executable).unwrap());
        let plan = Arc::new(
            check_plan(
                Profile::Enforce,
                PolicyControls::default(),
                execution(&repositories, case.status(), bootstrap_digest),
            )
            .unwrap(),
        );
        let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1)).unwrap();
        let ingress = IngressPolicy::new(
            IngressLimits::new(1_024, 32, 32 * 1_024).unwrap(),
            replay,
            Duration::from_secs(2),
        )
        .unwrap();
        let route = DeliveryRoute {
            provider: provider(),
            trust_set: OpaqueId::new("gitlab-oidc".to_owned()).unwrap(),
            signed_time: SignedTimePolicy::Required(Duration::from_mins(5)),
        };
        let source = source();
        let current = case.refreshes(&repositories);
        let api = FakeGitLab::new(current.into_iter().map(Ok));
        let adapter: Arc<dyn ProviderAdapter> =
            Arc::new(GitLabMergeTrainAdapter::new(source, api.clone()));
        let mut plans = PlanRegistry::new();
        register_plan(
            &mut plans,
            PlanScope {
                provider: provider(),
                integration: policy().integration,
                repository: RepositoryIdentity::new(
                    HOST.to_owned(),
                    "acme".to_owned(),
                    "widget".to_owned(),
                )
                .unwrap(),
            },
            plan,
        )
        .unwrap();
        let lane = Arc::new(Lane {
            route,
            adapter,
            plans,
            ledger: FileLedgerConfig::new(Duration::from_secs(2), 64, replay).unwrap(),
            ledger_root,
            ingress,
            acquisition: repositories.acquisition(),
            executable: executable_for(case, &state, &executable),
            scratch,
            wall_timeout: case.wall_timeout(),
        });
        let router = evaluation_router(
            &EvaluationConfig {
                path: ENDPOINT.to_owned(),
                max_body_bytes: 1_024,
                max_headers: 32,
                max_header_bytes: 32 * 1_024,
                max_concurrent_evaluations: 2,
            },
            move |request| evaluate(&lane, request),
        )
        .unwrap();
        let token = sign(&claims(&repositories.commits().candidate));
        Self {
            _state: state,
            repositories,
            router,
            token,
            api,
        }
    }

    pub(super) async fn request(&self) -> StatusCode {
        self.request_with(&self.token, br#"{"merge_request_iid":42}"#)
            .await
    }

    pub(super) async fn request_with(&self, token: &str, body: &'static [u8]) -> StatusCode {
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(ENDPOINT)
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    pub(super) fn claims(&self) -> Value {
        claims(&self.repositories.commits().candidate)
    }
}

fn evaluate(lane: &Lane, request: amiss_controller_service::EvaluationRequest<'_>) -> StatusCode {
    let headers = request
        .headers
        .iter()
        .map(|header| DeliveryHeader {
            name: &header.name,
            value: &header.value,
        })
        .collect::<Vec<_>>();
    let untrusted = UntrustedDelivery {
        route: &lane.route,
        received_at_unix_millis: request.received_at_unix_millis,
        headers: &headers,
        body: request.body,
    };
    let admitted = check_lane(&lane.ingress, &lane.plans, untrusted, |checked| {
        lane.adapter
            .authenticate(checked)
            .map_err(|_defect| AdmissionRejection::Unauthorized)
    });
    if let Err(rejection) = admitted {
        return rejection_status(rejection);
    }
    execute(lane, untrusted).map_or(StatusCode::SERVICE_UNAVAILABLE, |outcome| {
        if policy_job_accepted(&outcome) {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::PRECONDITION_FAILED
        }
    })
}

fn execute(
    lane: &Lane,
    untrusted: UntrustedDelivery<'_>,
) -> Result<amiss_controller::HandleOutcome, ()> {
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let ledger = FileLedger::open_with_clock(&lane.ledger_root, lane.ledger, Arc::clone(&clock))
        .map_err(|_defect| ())?;
    let runner = AcquiringRunner::new(
        lane.acquisition.clone(),
        lane.executable.clone(),
        lane.scratch.clone(),
        lane.wall_timeout,
        Duration::from_mins(5),
        Arc::clone(&clock),
    )
    .ok_or(())?;
    let mut adapters = AdapterRegistry::new();
    adapters
        .register(Arc::clone(&lane.adapter))
        .map_err(|_defect| ())?;
    Controller::new_with_clock(
        adapters,
        lane.plans.clone(),
        ledger,
        runner,
        lane.ingress,
        clock,
    )
    .handle(untrusted)
    .map_err(|_defect| ())
}

impl LaneCase {
    fn status(self) -> &'static str {
        match self {
            Self::Block => "runner-block",
            Self::MissingOutput => "runner-missing",
            Self::Timeout => "runner-hang",
            Self::Pass
            | Self::WrongTree
            | Self::WrongParents
            | Self::TamperedBootstrap
            | Self::FinalPolicyRevoked
            | Self::FinalGateChanged => "runner-pass",
        }
    }

    const fn wall_timeout(self) -> Duration {
        if matches!(self, Self::Timeout) {
            Duration::from_millis(50)
        } else {
            Duration::from_secs(1)
        }
    }

    fn refreshes(self, repositories: &Repositories) -> [amiss_controller_gitlab::GitLabRefresh; 3] {
        let first = refresh(repositories);
        let mut second = first.clone();
        let mut third = first.clone();
        match self {
            Self::WrongTree => {
                "dddddddddddddddddddddddddddddddddddddddd".clone_into(&mut second.gate.tree);
                second.clone_into(&mut third);
                [second.clone(), second, third]
            }
            Self::WrongParents => {
                second.gate.parents.pop();
                second.clone_into(&mut third);
                [second.clone(), second, third]
            }
            Self::FinalPolicyRevoked => {
                if let Some(protection) = third.protections.first_mut() {
                    protection.allow_force_push = true;
                }
                [first, second, third]
            }
            Self::FinalGateChanged => {
                "dddddddddddddddddddddddddddddddddddddddd".clone_into(&mut third.pipeline.sha);
                [first, second, third]
            }
            Self::Pass
            | Self::Block
            | Self::MissingOutput
            | Self::Timeout
            | Self::TamperedBootstrap => [first, second, third],
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
    input.action_repository = RepositoryIdentity::new(
        HOST.to_owned(),
        "security".to_owned(),
        "amiss-action".to_owned(),
    )
    .unwrap();
    input.action_object_format = ObjectFormat::Sha1;
    input.action_commit_oid = repositories.action_commit();
    input.action_tree_oid = repositories.action_tree();
    status.clone_into(&mut input.required_status_name);
    input.bootstrap_digest = bootstrap_digest;
    ExecutionConstraintDescriptor::new(input).unwrap()
}

fn executable_for(case: LaneCase, state: &TempDir, executable: &std::path::Path) -> PathBuf {
    if !matches!(case, LaneCase::TamperedBootstrap) {
        return executable.to_path_buf();
    }
    let changed = state.path().join("changed-bootstrap");
    std::fs::write(&changed, b"changed after the plan was fixed").unwrap();
    changed
}

const fn rejection_status(rejection: AdmissionRejection) -> StatusCode {
    match rejection {
        AdmissionRejection::Malformed => StatusCode::BAD_REQUEST,
        AdmissionRejection::Unauthorized => StatusCode::UNAUTHORIZED,
        AdmissionRejection::Forbidden => StatusCode::FORBIDDEN,
    }
}

fn directory(root: &TempDir, name: &str) -> PathBuf {
    let path = root.path().join(name);
    std::fs::create_dir(&path).unwrap();
    path
}

fn _exact_oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}
