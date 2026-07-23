use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller::{
    AcquiringRunner, AdapterRegistry, Controller, ControllerClock, DeliveryRoute, FileLedger,
    FileLedgerConfig, IngressPolicy, PlanRegistry, ProviderAdapter, ProviderError,
    ProviderIdentity, SystemClock, register_plan,
};
use amiss_controller_github::{
    GitFetchBounds, GitHubAcquisition, GitHubApp, GitHubPullRequestAdapter,
    GitHubPullRequestSource, GitHubTimeouts,
};
pub use amiss_controller_service::QueuedServiceError as ServiceError;
use amiss_controller_service::{
    AdmissionRejection, DeliveryAdmission, DeliveryWorker, DeliveryWorkerInput, Inbox,
    QueuedServiceInput, lane_admission, run_queued_service,
};
use amiss_wire::model::BranchRef;

use crate::config::ServiceConfig;

type GitHubWorker = DeliveryWorker<FileLedger, AcquiringRunner<GitHubAcquisition<GitHubApp>>>;

struct PreparedLane {
    service: QueuedServiceInput,
    admission: Arc<dyn DeliveryAdmission>,
    worker: WorkerContext,
}

struct WorkerContext {
    settings: WorkerSettings,
    bounds: GitFetchBounds,
    source: Arc<GitHubPullRequestSource>,
    plans: PlanRegistry,
    ledger: FileLedger,
    admission: Arc<dyn DeliveryAdmission>,
}

struct WorkerSettings {
    provider: ProviderIdentity,
    app_id: u64,
    installation_id: u64,
    private_key: Vec<u8>,
    api_base: String,
    required_status_name: String,
    api_timeouts: GitHubTimeouts,
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

/// Runs one configured GitHub App lane until shutdown or a fatal local error.
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
    let source = Arc::new(GitHubPullRequestSource::new(
        config.provider.clone(),
        config.webhook,
    ));
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, config.scope.clone(), Arc::clone(&config.plan))
        .map_err(|_defect| ServiceError("check plan cannot be registered"))?;
    let admission = admission(
        &source,
        config.target.clone(),
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
            app_id: config.app_id,
            installation_id: config.installation_id,
            private_key: config.private_key,
            api_base: config.api_base,
            required_status_name: config.plan.execution.required_status_name.clone(),
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
    source: &Arc<GitHubPullRequestSource>,
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
) -> Result<GitHubWorker, ServiceError> {
    let settings = input.settings;
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let app = GitHubApp::new(
        settings.provider,
        settings.app_id,
        settings.installation_id,
        settings.private_key,
        &settings.api_base,
        settings.required_status_name,
        settings.api_timeouts,
    )
    .map_err(|_defect| ServiceError("GitHub App client cannot start"))?;
    let adapter = Arc::new(GitHubPullRequestAdapter::from_source(
        input.source,
        app.clone(),
    ));
    let acquisition = GitHubAcquisition::new(app, input.bounds);
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
        .map_err(|_defect| ServiceError("GitHub adapter cannot be registered"))?;
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
