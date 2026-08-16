use std::sync::{Arc, Mutex};

use amiss_controller::{
    AcquiringRunner, ControllerClock, FileLedger, ProviderAdapter, SystemClock,
};
use amiss_controller_github::{
    GitFetchBounds, GitHubAcquisition, GitHubApp, GitHubPullRequestAdapter, GitHubPullRequestSource,
};
use amiss_controller_service::{
    AcquiringWorkerBuildError, AcquiringWorkerContext, AcquiringWorkerSettings, DeliveryAdmission,
    DeliveryWorker, Inbox, Operations, QueuedLaneSetup, QueuedLaneSetupError, QueuedLaneSetupInput,
    QueuedServiceError, QueuedServiceInput, acquiring_worker, repository_admission,
    run_queued_service, setup_queued_lane,
};

use crate::config::ServiceConfig;

type GitHubWorker = DeliveryWorker<FileLedger, AcquiringRunner<GitHubAcquisition<GitHubApp>>>;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Git acquisition timeout is invalid")]
    InvalidGitTimeout,
    #[error(transparent)]
    Setup(#[from] QueuedLaneSetupError),
    #[error(transparent)]
    Queued(#[from] QueuedServiceError),
}

struct PreparedLane {
    service: QueuedServiceInput,
    admission: Arc<dyn DeliveryAdmission>,
    worker: WorkerContext,
}

struct WorkerContext {
    app: GitHubApp,
    worker: AcquiringWorkerContext<FileLedger>,
    bounds: GitFetchBounds,
    source: Arc<GitHubPullRequestSource>,
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
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let QueuedLaneSetup {
        service,
        plans,
        ledger,
    } = setup_queued_lane(QueuedLaneSetupInput {
        service: QueuedServiceInput {
            listen: config.listen,
            receiver: config.receiver,
            inbox_root: config.inbox_root,
            inbox_limits: config.inbox,
            clock: Arc::clone(&clock),
        },
        plan: Arc::clone(&config.plan),
        scope: config.scope.clone(),
        ledger_root: config.ledger_root,
        ledger_lease: config.ledger_lease,
        ledger_records: config.ledger_records,
        replay: config.replay,
    })?;
    let admission_source = Arc::clone(&source);
    let target = config.target;
    let admission = repository_admission(
        config.route_id.clone(),
        config.route.clone(),
        config.ingress,
        plans.clone(),
        Arc::clone(&clock),
        config.repository_id,
        move |checked| admission_source.authenticate_for_target(checked, &target),
    );
    let worker = WorkerContext {
        app: config.app,
        worker: AcquiringWorkerContext {
            settings: AcquiringWorkerSettings {
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
            plans,
            ledger,
            admission: Arc::clone(&admission),
            clock,
        },
        bounds,
        source,
    };
    Ok(PreparedLane {
        service,
        admission,
        worker,
    })
}

fn build_worker(
    input: WorkerContext,
    inbox: Arc<Mutex<Inbox>>,
    operations: Operations,
) -> Result<GitHubWorker, AcquiringWorkerBuildError> {
    let app = input.app;
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(GitHubPullRequestAdapter::from_source(
        input.source,
        app.clone(),
    ));
    let acquisition = GitHubAcquisition::new(app, input.bounds);
    acquiring_worker(input.worker, inbox, operations, adapter, acquisition)
}
