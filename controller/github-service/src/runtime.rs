use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller::{
    AcquiringRunner, AdapterRegistry, Controller, ControllerClock, DeliveryRoute, FileLedger,
    FileLedgerConfig, FileLedgerError, IngressPolicy, PlanError, PlanRegistry, ProviderAdapter,
    ProviderError, RegistryError, SystemClock, register_plan,
};
use amiss_controller_github::{
    GitFetchBounds, GitHubAcquisition, GitHubApp, GitHubPullRequestAdapter, GitHubPullRequestSource,
};
use amiss_controller_service::{
    AdmissionRejection, DeliveryAdmission, DeliveryWorker, DeliveryWorkerError,
    DeliveryWorkerInput, Inbox, Operations, QueuedServiceError, QueuedServiceInput, lane_admission,
    run_queued_service,
};
use amiss_wire::model::BranchRef;

use crate::config::ServiceConfig;

type GitHubWorker = DeliveryWorker<FileLedger, AcquiringRunner<GitHubAcquisition<GitHubApp>>>;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Git acquisition timeout is invalid")]
    InvalidGitTimeout,
    #[error("check plan cannot be registered")]
    Plan(#[source] PlanError),
    #[error("delivery record limits are invalid")]
    InvalidLedgerLimits,
    #[error("delivery record cannot be opened")]
    Ledger(#[source] FileLedgerError),
    #[error(transparent)]
    Queued(#[from] QueuedServiceError),
}

#[derive(Debug, thiserror::Error)]
enum WorkerBuildError {
    #[error("bootstrap runner limits are invalid")]
    InvalidRunnerLimits,
    #[error("GitHub adapter cannot be registered")]
    Registry(#[source] RegistryError),
    #[error("delivery worker cannot start")]
    Worker(#[source] DeliveryWorkerError),
}

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
    app: GitHubApp,
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
    run_queued_service(service, admission, move |inbox, operations| {
        build_worker(worker, inbox, operations).map_err(QueuedServiceError::worker_build)
    })
    .await?;
    Ok(())
}

fn prepare(config: ServiceConfig) -> Result<PreparedLane, ServiceError> {
    let bounds = GitFetchBounds::new(config.git_timeout).ok_or(ServiceError::InvalidGitTimeout)?;
    let source = Arc::new(GitHubPullRequestSource::new(
        config.provider.clone(),
        config.webhook,
    ));
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, config.scope.clone(), Arc::clone(&config.plan))
        .map_err(ServiceError::Plan)?;
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let admission = admission(
        &source,
        config.target.clone(),
        config.repository_id,
        config.route_id.clone(),
        config.route.clone(),
        config.ingress,
        plans.clone(),
        Arc::clone(&clock),
    );
    let ledger_config =
        FileLedgerConfig::new(config.ledger_lease, config.ledger_records, config.replay)
            .ok_or(ServiceError::InvalidLedgerLimits)?;
    let ledger =
        FileLedger::open(&config.ledger_root, ledger_config).map_err(ServiceError::Ledger)?;
    let service = QueuedServiceInput {
        listen: config.listen,
        receiver: config.receiver,
        inbox_root: config.inbox_root,
        inbox_limits: config.inbox,
        clock,
    };
    let worker = WorkerContext {
        settings: WorkerSettings {
            app: config.app,
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

#[expect(
    clippy::too_many_arguments,
    reason = "one lane's route, ingress, plans, and clock all bind here"
)]
fn admission(
    source: &Arc<GitHubPullRequestSource>,
    target: BranchRef,
    repository_id: u64,
    route_id: String,
    route: DeliveryRoute,
    ingress: IngressPolicy,
    plans: PlanRegistry,
    clock: Arc<dyn ControllerClock>,
) -> Arc<dyn DeliveryAdmission> {
    let source = Arc::clone(source);
    let repository_prefix = format!("repository/{repository_id}/");
    Arc::new(lane_admission(
        route_id,
        route,
        ingress,
        plans,
        clock,
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
    operations: Operations,
) -> Result<GitHubWorker, WorkerBuildError> {
    let settings = input.settings;
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let app = settings.app;
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
    .ok_or(WorkerBuildError::InvalidRunnerLimits)?;
    let mut registry = AdapterRegistry::new();
    let registered: Arc<dyn ProviderAdapter> = adapter;
    registry
        .register(registered)
        .map_err(WorkerBuildError::Registry)?;
    let controller = Controller::new_with_clock(
        registry,
        input.plans,
        input.ledger,
        runner,
        settings.ingress,
        Arc::clone(&clock),
    )
    .with_external_sink(Arc::new(operations.clone()));
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
        operations,
    })
    .map_err(WorkerBuildError::Worker)
}
