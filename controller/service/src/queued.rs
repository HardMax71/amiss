use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use amiss_controller::{ControllerClock, DeliveryLedger, Runner};
use tokio::net::TcpListener;

use crate::{
    DeliveryAdmission, DeliveryWorker, Inbox, InboxLimits, Operations, ReceiverConfig,
    ServiceComponent, Supervision, SupervisionError, router_with_clock, supervise,
};

pub struct QueuedServiceInput {
    pub listen: SocketAddr,
    pub receiver: ReceiverConfig,
    pub inbox_root: PathBuf,
    pub inbox_limits: InboxLimits,
    pub clock: Arc<dyn ControllerClock>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedServiceError(pub &'static str);

impl fmt::Display for QueuedServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for QueuedServiceError {}

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
    let shutdown = crate::shutdown_signal()
        .map_err(|_defect| QueuedServiceError("shutdown signal handler cannot be installed"))?;
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
        Inbox::open(input.inbox_root, input.inbox_limits)
            .map_err(|_defect| QueuedServiceError("delivery inbox cannot be opened"))?,
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
    .map_err(|_defect| QueuedServiceError("HTTP receiver configuration is invalid"))?;
    let listener = TcpListener::bind(input.listen)
        .await
        .map_err(|_defect| QueuedServiceError("HTTP listener cannot bind"))?;
    let worker_operations = operations.clone();
    let worker_inbox = Arc::clone(&inbox);
    let worker = tokio::task::spawn_blocking(move || build_worker(worker_inbox, worker_operations))
        .await
        .map_err(|_panic| QueuedServiceError("delivery worker panicked"))??;
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let component = async move {
        tokio::task::spawn_blocking(move || worker.run(&worker_stop))
            .await
            .map_err(|_panic| SupervisionError("delivery worker panicked"))?
            .map_err(|_defect| SupervisionError("delivery worker stopped"))
    };
    let stop_component = move || {
        let stop_result = inbox
            .lock()
            .map(|_guard| stop.store(true, Ordering::Release))
            .map_err(|_poisoned| SupervisionError("delivery inbox lock is unavailable"));
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
    .map_err(|error| QueuedServiceError(error.0))
}
