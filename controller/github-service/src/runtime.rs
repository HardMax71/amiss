use std::sync::{Arc, Mutex};

use amiss_controller::{AcquiringRunner, FileLedger, ProviderAdapter, SystemClock};
use amiss_controller_github::{
    GitFetchBounds, GitHubAcquisition, GitHubApp, GitHubPullRequestAdapter, GitHubPullRequestSource,
};
use amiss_controller_service::{
    AcquiringWorkerBuildError, AcquiringWorkerContext, DeliveryWorker, Inbox, Operations,
    QueuedLaneSetupError, QueuedService, QueuedServiceError, acquiring_worker, run_queued_service,
    setup_repository_lane,
};

use crate::config::ServiceConfig;

type GitHubWorker = DeliveryWorker<FileLedger, AcquiringRunner<GitHubAcquisition<GitHubApp>>>;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Git acquisition timeout is invalid")]
    InvalidGitTimeout,
    #[error(transparent)]
    Setup(QueuedLaneSetupError),
    #[error(transparent)]
    Queued(QueuedServiceError),
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
    let service = prepare(config)?;
    run_queued_service(service, build_worker)
        .await
        .map_err(ServiceError::Queued)
}

fn prepare(config: ServiceConfig) -> Result<QueuedService<WorkerContext>, ServiceError> {
    let bounds = GitFetchBounds::new(config.git_timeout).ok_or(ServiceError::InvalidGitTimeout)?;
    let source = Arc::new(GitHubPullRequestSource::new(
        config.provider.clone(),
        config.webhook,
    ));
    let admission_source = Arc::clone(&source);
    let target = config.target;
    let queued = setup_repository_lane(
        config.lane,
        config.worker,
        config.repository_id,
        Arc::new(SystemClock),
        move |checked| admission_source.authenticate_for_target(checked, &target),
    )
    .map_err(ServiceError::Setup)?;
    Ok(QueuedService {
        settings: queued.settings,
        clock: queued.clock,
        admission: queued.admission,
        artifacts: queued.artifacts,
        worker: WorkerContext {
            app: config.app,
            worker: queued.worker,
            bounds,
            source,
        },
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
