use std::sync::{Arc, Mutex};

use amiss_controller::{
    AcquiringRunner, FileLedger, ProviderAdapter, ProviderIdentity, RunRequest, SystemClock,
};
use amiss_controller_git::{GitAcquisition, GitAcquisitionPlan, GitFetchBounds, GitRemote};
use amiss_controller_gitea::{
    DedicatedReviewer, GiteaClient, GiteaClientError, GiteaFetchPlan, GiteaObjectResolver,
    GiteaPlanError, GiteaPullRequestAdapter, GiteaPullRequestSource, GiteaTimeouts,
    gitea_fetch_plan,
};
use amiss_controller_service::{
    AcquiringWorkerBuildError, AcquiringWorkerContext, DeliveryWorker, Inbox, Operations,
    QueuedLaneSetupError, QueuedService, QueuedServiceError, acquiring_worker, run_queued_service,
    setup_repository_lane,
};
use secrecy::{ExposeSecret as _, SecretString};

use crate::config::ServiceConfig;

type PlanBuilder = Box<dyn FnMut(&RunRequest) -> Result<GitAcquisitionPlan, GiteaPlanError> + Send>;
type GiteaAcquisition = GitAcquisition<PlanBuilder>;
type GiteaWorker = DeliveryWorker<FileLedger, AcquiringRunner<GiteaAcquisition>>;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Git acquisition timeout is invalid")]
    InvalidGitTimeout,
    #[error("Gitea-family webhook source cannot be created")]
    InvalidWebhookSource,
    #[error(transparent)]
    Setup(QueuedLaneSetupError),
    #[error(transparent)]
    Queued(QueuedServiceError),
}

#[derive(Debug, thiserror::Error)]
enum WorkerBuildError {
    #[error("Gitea-family client cannot start")]
    Client(#[source] GiteaClientError),
    #[error(transparent)]
    Shared(AcquiringWorkerBuildError),
}

struct WorkerContext {
    settings: WorkerSettings,
    worker: AcquiringWorkerContext<FileLedger>,
    bounds: GitFetchBounds,
    source: Arc<GiteaPullRequestSource>,
}

struct WorkerSettings {
    provider: ProviderIdentity,
    reviewer: DedicatedReviewer,
    token: SecretString,
    api_base: String,
    objects: Arc<dyn GiteaObjectResolver>,
    review_name: String,
    api_timeouts: GiteaTimeouts,
}

/// Runs one configured Gitea-family dedicated-reviewer lane until shutdown.
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
    let source = Arc::new(
        GiteaPullRequestSource::new(
            config.provider.clone(),
            config.reviewer.clone(),
            config.webhook,
        )
        .ok_or(ServiceError::InvalidWebhookSource)?,
    );
    let admission_source = Arc::clone(&source);
    let target = config.target;
    let queued = setup_repository_lane(
        config.lane,
        config.worker,
        config.repository_id,
        Arc::new(SystemClock),
        move |checked| {
            admission_source
                .authenticate_for_target(checked, &target)
                .map(Some)
        },
    )
    .map_err(ServiceError::Setup)?;
    Ok(QueuedService {
        settings: queued.settings,
        clock: queued.clock,
        admission: queued.admission,
        worker: WorkerContext {
            settings: WorkerSettings {
                provider: config.provider,
                reviewer: config.reviewer,
                token: config.token,
                api_base: config.api_base,
                objects: config.objects,
                review_name: config.review_name,
                api_timeouts: config.api_timeouts,
            },
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
) -> Result<GiteaWorker, WorkerBuildError> {
    let settings = input.settings;
    let client = GiteaClient::new(
        settings.provider,
        settings.reviewer.clone(),
        settings.token.expose_secret().to_owned(),
        &settings.api_base,
        settings.review_name,
        settings.api_timeouts,
        settings.objects,
    )
    .map_err(WorkerBuildError::Client)?;
    let adapter: Arc<dyn ProviderAdapter> =
        Arc::new(GiteaPullRequestAdapter::from_source(input.source, client));
    let acquisition = git_acquisition(input.bounds, settings.reviewer, settings.token);
    acquiring_worker(input.worker, inbox, operations, adapter, acquisition)
        .map_err(WorkerBuildError::Shared)
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
