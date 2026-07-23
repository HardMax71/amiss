use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use amiss_controller::{DeliveryLedger, Runner};
use tokio::net::TcpListener;

use crate::{DeliveryAdmission, DeliveryWorker, Inbox, InboxLimits, ReceiverConfig, router, serve};

pub struct QueuedServiceInput {
    pub listen: SocketAddr,
    pub receiver: ReceiverConfig,
    pub inbox_root: PathBuf,
    pub inbox_limits: InboxLimits,
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
    F: FnOnce(Arc<Mutex<Inbox>>) -> Result<DeliveryWorker<L, R>, QueuedServiceError>
        + Send
        + 'static,
{
    let inbox = Arc::new(Mutex::new(
        Inbox::open(input.inbox_root, input.inbox_limits)
            .map_err(|_defect| QueuedServiceError("delivery inbox cannot be opened"))?,
    ));
    let receiver = router(&input.receiver, Arc::clone(&inbox), admission)
        .map_err(|_defect| QueuedServiceError("HTTP receiver configuration is invalid"))?;
    let listener = TcpListener::bind(input.listen)
        .await
        .map_err(|_defect| QueuedServiceError("HTTP listener cannot bind"))?;
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let mut worker_task = tokio::task::spawn_blocking(move || {
        build_worker(inbox)?
            .run(&worker_stop)
            .map_err(|_defect| QueuedServiceError("delivery worker stopped"))
    });
    let mut server = Box::pin(serve(listener, receiver));
    tokio::select! {
        result = &mut server => {
            stop.store(true, Ordering::Release);
            let worker_result = worker_task.await;
            result.map_err(|_defect| QueuedServiceError("HTTP receiver stopped"))?;
            join_worker(worker_result)
        }
        result = &mut worker_task => join_worker(result),
        signal = crate::shutdown_signal() => {
            signal.map_err(|_defect| QueuedServiceError("shutdown signal cannot be observed"))?;
            stop.store(true, Ordering::Release);
            join_worker(worker_task.await)
        }
    }
}

fn join_worker(
    result: Result<Result<(), QueuedServiceError>, tokio::task::JoinError>,
) -> Result<(), QueuedServiceError> {
    match result {
        Ok(result) => result,
        Err(_panic) => Err(QueuedServiceError("delivery worker panicked")),
    }
}
