use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller::{
    CheckPlan, ControllerClock, DeliveryLedger, FileLedger, FileLedgerConfig, FileLedgerError,
    IngressCheck, PlanError, PlanRegistry, PlanScope, ProviderError, ReplayWindow, Runner,
    VerifiedDelivery, register_plan,
};
use tokio::net::TcpListener;

use crate::{
    AcquiringWorkerContext, AcquiringWorkerSettings, DeliveryAdmission, DeliveryWorker, Inbox,
    InboxError, InboxLimits, Operations, ReceiverConfig, ReceiverConfigError, ServiceComponent,
    Supervision, SupervisionError, repository_admission, router_with_clock, supervise,
};

pub struct QueuedServiceSettings {
    pub listen: SocketAddr,
    pub receiver: ReceiverConfig,
    pub inbox_root: PathBuf,
    pub inbox_limits: InboxLimits,
}

pub struct QueuedService<W> {
    pub settings: QueuedServiceSettings,
    pub clock: Arc<dyn ControllerClock>,
    pub admission: Arc<dyn DeliveryAdmission>,
    pub worker: W,
}

pub struct QueuedLaneSetupInput {
    pub service: QueuedServiceSettings,
    pub plan: Arc<CheckPlan>,
    pub scope: PlanScope,
    pub ledger_root: PathBuf,
    pub ledger_lease: Duration,
    pub ledger_records: u64,
    pub replay: ReplayWindow,
}

#[derive(Debug, thiserror::Error)]
pub enum QueuedLaneSetupError {
    #[error("check plan cannot be registered")]
    Plan(#[source] PlanError),
    #[error("delivery record limits are invalid")]
    InvalidLedgerLimits,
    #[error("delivery record cannot be opened")]
    Ledger(#[source] FileLedgerError),
}

/// Opens one queued repository lane and binds its admission and worker state to one clock.
///
/// # Errors
///
/// The plan conflicts, the ledger limits are invalid, or the ledger cannot be opened.
pub fn setup_repository_lane<F>(
    input: QueuedLaneSetupInput,
    settings: AcquiringWorkerSettings,
    repository_id: u64,
    clock: Arc<dyn ControllerClock>,
    authenticate: F,
) -> Result<QueuedService<AcquiringWorkerContext<FileLedger>>, QueuedLaneSetupError>
where
    F: for<'a> Fn(IngressCheck<'a>) -> Result<VerifiedDelivery, ProviderError>
        + Send
        + Sync
        + 'static,
{
    let QueuedLaneSetupInput {
        service,
        plan,
        scope,
        ledger_root,
        ledger_lease,
        ledger_records,
        replay,
    } = input;
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, scope, plan).map_err(QueuedLaneSetupError::Plan)?;
    let ledger_config = FileLedgerConfig::new(ledger_lease, ledger_records, replay)
        .ok_or(QueuedLaneSetupError::InvalidLedgerLimits)?;
    let ledger = FileLedger::open_with_clock(&ledger_root, ledger_config, Arc::clone(&clock))
        .map_err(QueuedLaneSetupError::Ledger)?;
    let admission = repository_admission(
        settings.route_id.clone(),
        settings.route.clone(),
        settings.ingress,
        plans.clone(),
        Arc::clone(&clock),
        repository_id,
        authenticate,
    );
    let worker = AcquiringWorkerContext {
        settings,
        plans,
        ledger,
        admission: Arc::clone(&admission),
        clock: Arc::clone(&clock),
    };
    Ok(QueuedService {
        settings: service,
        clock,
        admission,
        worker,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum QueuedServiceError {
    #[error("shutdown signal handler cannot be installed")]
    ShutdownInstall(#[source] std::io::Error),
    #[error("delivery inbox cannot be opened")]
    InboxOpen(#[source] InboxError),
    #[error("HTTP receiver configuration is invalid")]
    Receiver(#[source] ReceiverConfigError),
    #[error("HTTP listener cannot bind")]
    Listener(#[source] std::io::Error),
    #[error("delivery worker panicked")]
    WorkerPanicked(#[source] tokio::task::JoinError),
    #[error("{0}")]
    WorkerBuild(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Supervision(SupervisionError),
}

/// Runs one durable receiver and its blocking delivery worker until shutdown.
///
/// # Errors
///
/// The inbox, receiver, listener, worker, server, or shutdown signal fails.
pub async fn run_queued_service<L, R, W, F, E>(
    service: QueuedService<W>,
    build_worker: F,
) -> Result<(), QueuedServiceError>
where
    L: DeliveryLedger + Send + 'static,
    R: Runner + Send + 'static,
    W: Send + 'static,
    F: FnOnce(W, Arc<Mutex<Inbox>>, Operations) -> Result<DeliveryWorker<L, R>, E> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    let shutdown = crate::shutdown_signal().map_err(QueuedServiceError::ShutdownInstall)?;
    run_queued_service_until(service, build_worker, Operations::default(), shutdown).await
}

async fn run_queued_service_until<L, R, W, F, E, S>(
    service: QueuedService<W>,
    build_worker: F,
    operations: Operations,
    shutdown: S,
) -> Result<(), QueuedServiceError>
where
    L: DeliveryLedger + Send + 'static,
    R: Runner + Send + 'static,
    W: Send + 'static,
    F: FnOnce(W, Arc<Mutex<Inbox>>, Operations) -> Result<DeliveryWorker<L, R>, E> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
    S: Future<Output = std::io::Result<()>>,
{
    let QueuedService {
        settings,
        clock,
        admission,
        worker,
    } = service;
    let inbox = Arc::new(Mutex::new(
        Inbox::open(settings.inbox_root, settings.inbox_limits)
            .map_err(QueuedServiceError::InboxOpen)?,
    ));
    let ready = Arc::new(AtomicBool::new(false));
    let (receiver, endpoint) = router_with_clock(
        &settings.receiver,
        Arc::clone(&inbox),
        admission,
        Arc::clone(&ready),
        operations.clone(),
        Arc::clone(&clock),
    )
    .map_err(QueuedServiceError::Receiver)?;
    let listener = TcpListener::bind(settings.listen)
        .await
        .map_err(QueuedServiceError::Listener)?;
    let worker_operations = operations.clone();
    let worker_inbox = Arc::clone(&inbox);
    let worker =
        tokio::task::spawn_blocking(move || build_worker(worker, worker_inbox, worker_operations))
            .await
            .map_err(QueuedServiceError::WorkerPanicked)?
            .map_err(|error| QueuedServiceError::WorkerBuild(Box::new(error)))?;
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let component = async move {
        tokio::task::spawn_blocking(move || worker.run(&worker_stop))
            .await
            .map_err(SupervisionError::WorkerPanicked)?
            .map_err(SupervisionError::Worker)
    };
    let stop_component = move || {
        let stop_result = inbox
            .lock()
            .map(|_guard| stop.store(true, Ordering::Release))
            .map_err(|_poisoned| SupervisionError::InboxLock);
        if stop_result.is_err() {
            stop.store(true, Ordering::Release);
        }
        stop_result
    };
    supervise(
        Supervision {
            listener,
            router: receiver,
            ready,
            operations,
            endpoint,
            component: ServiceComponent::Worker,
        },
        component,
        shutdown,
        stop_component,
    )
    .await
    .map_err(QueuedServiceError::Supervision)
}
