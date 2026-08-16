use std::sync::{Arc, Mutex};

use amiss_controller::{
    AcquiringRunner, ControllerClock, FileLedger, ProviderAdapter, ProviderIdentity, RunRequest,
    SystemClock,
};
use amiss_controller_git::{GitAcquisition, GitAcquisitionPlan, GitFetchBounds, GitRemote};
use amiss_controller_gitea::{
    DedicatedReviewer, GiteaClient, GiteaClientError, GiteaFetchPlan, GiteaObjectResolver,
    GiteaPlanError, GiteaPullRequestAdapter, GiteaPullRequestSource, GiteaTimeouts,
    gitea_fetch_plan,
};
use amiss_controller_service::{
    AcquiringWorkerBuildError, AcquiringWorkerContext, AcquiringWorkerSettings, DeliveryAdmission,
    DeliveryWorker, Inbox, Operations, QueuedLaneSetup, QueuedLaneSetupError, QueuedLaneSetupInput,
    QueuedServiceError, QueuedServiceInput, acquiring_worker, repository_admission,
    run_queued_service, setup_queued_lane,
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
    Setup(#[from] QueuedLaneSetupError),
    #[error(transparent)]
    Queued(#[from] QueuedServiceError),
}

#[derive(Debug, thiserror::Error)]
enum WorkerBuildError {
    #[error("Gitea-family client cannot start")]
    Client(#[source] GiteaClientError),
    #[error(transparent)]
    Shared(#[from] AcquiringWorkerBuildError),
}

struct PreparedLane {
    service: QueuedServiceInput,
    admission: Arc<dyn DeliveryAdmission>,
    worker: WorkerContext,
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
    let source = Arc::new(
        GiteaPullRequestSource::new(
            config.provider.clone(),
            config.reviewer.clone(),
            config.webhook,
        )
        .ok_or(ServiceError::InvalidWebhookSource)?,
    );
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
        settings: WorkerSettings {
            provider: config.provider,
            reviewer: config.reviewer,
            token: config.token,
            api_base: config.api_base,
            objects: config.objects,
            review_name: config.plan.execution.required_status_name().to_owned(),
            api_timeouts: config.api_timeouts,
        },
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
    Ok(acquiring_worker(
        input.worker,
        inbox,
        operations,
        adapter,
        acquisition,
    )?)
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
