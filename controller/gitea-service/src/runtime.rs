use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller::{
    AcquiringRunner, AdapterRegistry, Controller, ControllerClock, DeliveryRoute, FileLedger,
    FileLedgerConfig, IngressPolicy, PlanRegistry, ProviderAdapter, ProviderError,
    ProviderIdentity, RunRequest, SystemClock, register_plan,
};
use amiss_controller_git::{GitAcquisition, GitAcquisitionPlan, GitFetchBounds, GitRemote};
use amiss_controller_gitea::{
    DedicatedReviewer, GiteaClient, GiteaFetchPlan, GiteaPlanError, GiteaPullRequestAdapter,
    GiteaPullRequestSource, GiteaTimeouts, gitea_fetch_plan,
};
pub use amiss_controller_service::QueuedServiceError as ServiceError;
use amiss_controller_service::{
    AdmissionRejection, DeliveryAdmission, DeliveryWorker, DeliveryWorkerInput, Inbox,
    QueuedServiceInput, lane_admission, run_queued_service,
};
use amiss_wire::model::BranchRef;
use secrecy::{ExposeSecret as _, SecretString};

use crate::config::ServiceConfig;

type PlanBuilder = Box<dyn FnMut(&RunRequest) -> Result<GitAcquisitionPlan, GiteaPlanError> + Send>;
type GiteaAcquisition = GitAcquisition<PlanBuilder>;
type GiteaWorker = DeliveryWorker<FileLedger, AcquiringRunner<GiteaAcquisition>>;

struct PreparedLane {
    service: QueuedServiceInput,
    admission: Arc<dyn DeliveryAdmission>,
    worker: WorkerContext,
}

struct WorkerContext {
    settings: WorkerSettings,
    bounds: GitFetchBounds,
    source: Arc<GiteaPullRequestSource>,
    plans: PlanRegistry,
    ledger: FileLedger,
    admission: Arc<dyn DeliveryAdmission>,
}

struct WorkerSettings {
    provider: ProviderIdentity,
    reviewer: DedicatedReviewer,
    token: SecretString,
    api_base: String,
    review_name: String,
    api_timeouts: GiteaTimeouts,
    bootstrap: PathBuf,
    scratch: PathBuf,
    bootstrap_timeout: Duration,
    statement_validity: Duration,
    ingress: IngressPolicy,
    route: DeliveryRoute,
    route_id: String,
    retry_min: Duration,
    retry_max: Duration,
    idle_poll: Duration,
}

/// Runs one configured Gitea-family dedicated-reviewer lane until shutdown.
///
/// # Errors
///
/// A credential, state root, route, listener, worker, or controller invariant failed.
pub async fn run(config: ServiceConfig) -> Result<(), ServiceError> {
    let PreparedLane {
        service,
        admission,
        worker,
    } = prepare(config)?;
    run_queued_service(service, admission, move |inbox| build_worker(worker, inbox)).await
}

fn prepare(config: ServiceConfig) -> Result<PreparedLane, ServiceError> {
    let bounds = GitFetchBounds::new(config.git_timeout)
        .ok_or(ServiceError("Git acquisition timeout is invalid"))?;
    let source = Arc::new(
        GiteaPullRequestSource::new(
            config.provider.clone(),
            config.reviewer.clone(),
            config.webhook,
        )
        .ok_or(ServiceError(
            "Gitea-family webhook source cannot be created",
        ))?,
    );
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, config.scope.clone(), Arc::clone(&config.plan))
        .map_err(|_defect| ServiceError("check plan cannot be registered"))?;
    let admission = admission(
        &source,
        config.target,
        config.repository_id,
        config.route_id.clone(),
        config.route.clone(),
        config.ingress,
        plans.clone(),
    );
    let ledger_config =
        FileLedgerConfig::new(config.ledger_lease, config.ledger_records, config.replay)
            .ok_or(ServiceError("delivery record limits are invalid"))?;
    let ledger = FileLedger::open(&config.ledger_root, ledger_config)
        .map_err(|_defect| ServiceError("delivery record cannot be opened"))?;
    let service = QueuedServiceInput {
        listen: config.listen,
        receiver: config.receiver,
        inbox_root: config.inbox_root,
        inbox_limits: config.inbox,
    };
    let worker = WorkerContext {
        settings: WorkerSettings {
            provider: config.provider,
            reviewer: config.reviewer,
            token: config.token,
            api_base: config.api_base,
            review_name: config.plan.execution.required_status_name.clone(),
            api_timeouts: config.api_timeouts,
            bootstrap: config.bootstrap,
            scratch: config.scratch,
            bootstrap_timeout: config.bootstrap_timeout,
            statement_validity: config.statement_validity,
            ingress: config.ingress,
            route: config.route,
            route_id: config.route_id,
            retry_min: config.retry_min,
            retry_max: config.retry_max,
            idle_poll: config.idle_poll,
        },
        bounds,
        source,
        plans,
        ledger,
        admission: Arc::clone(&admission),
    };
    Ok(PreparedLane {
        service,
        admission,
        worker,
    })
}

fn admission(
    source: &Arc<GiteaPullRequestSource>,
    target: BranchRef,
    repository_id: u64,
    route_id: String,
    route: DeliveryRoute,
    ingress: IngressPolicy,
    plans: PlanRegistry,
) -> Arc<dyn DeliveryAdmission> {
    let source = Arc::clone(source);
    let repository_prefix = format!("repository/{repository_id}/");
    Arc::new(lane_admission(
        route_id,
        route,
        ingress,
        plans,
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
                .starts_with(&repository_prefix)
                .then_some(verified)
                .ok_or(AdmissionRejection::Forbidden)
        },
    ))
}

fn build_worker(
    input: WorkerContext,
    inbox: Arc<Mutex<Inbox>>,
) -> Result<GiteaWorker, ServiceError> {
    let settings = input.settings;
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let client = GiteaClient::new(
        settings.provider,
        settings.reviewer.clone(),
        settings.token.expose_secret().to_owned(),
        &settings.api_base,
        settings.review_name,
        settings.api_timeouts,
    )
    .map_err(|_defect| ServiceError("Gitea-family client cannot start"))?;
    let adapter = Arc::new(GiteaPullRequestAdapter::from_source(input.source, client));
    let acquisition = git_acquisition(input.bounds, settings.reviewer, settings.token);
    let runner = AcquiringRunner::new(
        acquisition,
        settings.bootstrap,
        settings.scratch,
        settings.bootstrap_timeout,
        settings.statement_validity,
        Arc::clone(&clock),
    )
    .ok_or(ServiceError("bootstrap runner limits are invalid"))?;
    let mut registry = AdapterRegistry::new();
    let registered: Arc<dyn ProviderAdapter> = adapter;
    registry
        .register(registered)
        .map_err(|_defect| ServiceError("Gitea-family adapter cannot be registered"))?;
    let controller = Controller::new_with_clock(
        registry,
        input.plans,
        input.ledger,
        runner,
        settings.ingress,
        Arc::clone(&clock),
    );
    DeliveryWorker::new(DeliveryWorkerInput {
        inbox,
        controller,
        admission: input.admission,
        route: settings.route,
        route_id: settings.route_id,
        retry_min: settings.retry_min,
        retry_max: settings.retry_max,
        idle_poll: settings.idle_poll,
        clock,
    })
    .map_err(|_defect| ServiceError("delivery worker cannot start"))
}

fn git_acquisition(
    bounds: GitFetchBounds,
    reviewer: DedicatedReviewer,
    token: SecretString,
) -> GiteaAcquisition {
    let build: PlanBuilder = Box::new(move |request| {
        let plan = gitea_fetch_plan(request)?;
        (plan.integration_id == reviewer.id)
            .then(|| acquisition_plan(plan, &reviewer.login, &token))
            .ok_or(GiteaPlanError::InvalidRequest)
    });
    GitAcquisition {
        bounds,
        plan: build,
    }
}

fn acquisition_plan(
    plan: GiteaFetchPlan,
    username: &str,
    token: &SecretString,
) -> GitAcquisitionPlan {
    GitAcquisitionPlan {
        repository: remote(plan.repository_url, username, token),
        repository_oids: plan.repository_oids,
        action: remote(plan.action_url, username, token),
        action_oid: plan.action_oid,
    }
}

fn remote(url: String, username: &str, token: &SecretString) -> GitRemote {
    GitRemote {
        url,
        username: username.to_owned(),
        password: SecretString::from(token.expose_secret().to_owned()),
    }
}
