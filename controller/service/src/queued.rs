use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use amiss_controller::{
    CheckPlan, ControllerClock, DeliveryLedger, FileLedger, FileLedgerConfig, FileLedgerError,
    PlanError, PlanRegistry, PlanScope, ReplayWindow, Runner, register_plan,
};
use tokio::net::TcpListener;

use crate::{
    DeliveryAdmission, DeliveryWorker, Inbox, InboxError, InboxLimits, Operations, ReceiverConfig,
    ReceiverConfigError, ServiceComponent, Supervision, SupervisionError, router_with_clock,
    supervise,
};

pub struct QueuedServiceInput {
    pub listen: SocketAddr,
    pub receiver: ReceiverConfig,
    pub inbox_root: PathBuf,
    pub inbox_limits: InboxLimits,
    pub clock: Arc<dyn ControllerClock>,
}

pub struct QueuedLaneSetupInput {
    pub service: QueuedServiceInput,
    pub plan: Arc<CheckPlan>,
    pub scope: PlanScope,
    pub ledger_root: PathBuf,
    pub ledger_lease: Duration,
    pub ledger_records: u64,
    pub replay: ReplayWindow,
}

pub struct QueuedLaneSetup {
    pub service: QueuedServiceInput,
    pub plans: PlanRegistry,
    pub ledger: FileLedger,
}

#[derive(Debug, thiserror::Error)]
pub enum QueuedLaneSetupError {
    #[error("check plan cannot be registered")]
    Plan(#[from] PlanError),
    #[error("delivery record limits are invalid")]
    InvalidLedgerLimits,
    #[error("delivery record cannot be opened")]
    Ledger(#[from] FileLedgerError),
}

/// Registers the lane plan and opens its file-backed delivery ledger.
///
/// # Errors
///
/// The plan conflicts, the ledger limits are invalid, or the ledger cannot be opened.
pub fn setup_queued_lane(
    input: QueuedLaneSetupInput,
) -> Result<QueuedLaneSetup, QueuedLaneSetupError> {
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, input.scope, input.plan)?;
    let ledger_config =
        FileLedgerConfig::new(input.ledger_lease, input.ledger_records, input.replay)
            .ok_or(QueuedLaneSetupError::InvalidLedgerLimits)?;
    let ledger = FileLedger::open_with_clock(
        &input.ledger_root,
        ledger_config,
        Arc::clone(&input.service.clock),
    )?;
    Ok(QueuedLaneSetup {
        service: input.service,
        plans,
        ledger,
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
    Supervision(#[from] SupervisionError),
}

impl QueuedServiceError {
    pub fn worker_build(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::WorkerBuild(Box::new(error))
    }
}

/// Runs one durable receiver and its blocking delivery worker until shutdown.
///
/// # Errors
///
/// The inbox, receiver, listener, worker, server, or shutdown signal fails.
pub async fn run_queued_service<L, R, F>(
    input: QueuedServiceInput,
    admission: Arc<dyn DeliveryAdmission>,
    build_worker: F,
) -> Result<(), QueuedServiceError>
where
    L: DeliveryLedger + Send + 'static,
    R: Runner + Send + 'static,
    F: FnOnce(Arc<Mutex<Inbox>>, Operations) -> Result<DeliveryWorker<L, R>, QueuedServiceError>
        + Send
        + 'static,
{
    let shutdown = crate::shutdown_signal().map_err(QueuedServiceError::ShutdownInstall)?;
    run_queued_service_until(
        input,
        admission,
        build_worker,
        Operations::default(),
        shutdown,
    )
    .await
}

async fn run_queued_service_until<L, R, F, S>(
    input: QueuedServiceInput,
    admission: Arc<dyn DeliveryAdmission>,
    build_worker: F,
    operations: Operations,
    shutdown: S,
) -> Result<(), QueuedServiceError>
where
    L: DeliveryLedger + Send + 'static,
    R: Runner + Send + 'static,
    F: FnOnce(Arc<Mutex<Inbox>>, Operations) -> Result<DeliveryWorker<L, R>, QueuedServiceError>
        + Send
        + 'static,
    S: Future<Output = std::io::Result<()>>,
{
    let inbox = Arc::new(Mutex::new(
        Inbox::open(input.inbox_root, input.inbox_limits).map_err(QueuedServiceError::InboxOpen)?,
    ));
    let ready = Arc::new(AtomicBool::new(false));
    let (receiver, endpoint) = router_with_clock(
        &input.receiver,
        Arc::clone(&inbox),
        admission,
        Arc::clone(&ready),
        operations.clone(),
        Arc::clone(&input.clock),
    )
    .map_err(QueuedServiceError::Receiver)?;
    let listener = TcpListener::bind(input.listen)
        .await
        .map_err(QueuedServiceError::Listener)?;
    let worker_operations = operations.clone();
    let worker_inbox = Arc::clone(&inbox);
    let worker = tokio::task::spawn_blocking(move || build_worker(worker_inbox, worker_operations))
        .await
        .map_err(QueuedServiceError::WorkerPanicked)??;
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
