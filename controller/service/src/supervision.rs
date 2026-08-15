mod tests;

use std::future::Future;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::probe::EndpointDrain;
use crate::receiver::serve;
use crate::{Operations, ServiceComponent, ServiceEvent};

pub struct Supervision {
    pub listener: TcpListener,
    pub router: Router,
    pub ready: Arc<AtomicBool>,
    pub operations: Operations,
    pub endpoint: EndpointDrain,
    pub component: ServiceComponent,
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisionError {
    #[error("service component stop panicked")]
    StopPanicked(#[source] tokio::task::JoinError),
    #[error("HTTP service stopped")]
    HttpStopped(#[source] Option<io::Error>),
    #[error("service component stopped")]
    ComponentStopped,
    #[error("{0}")]
    Component(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("HTTP endpoint cannot drain")]
    EndpointDrain(#[source] tokio::sync::AcquireError),
    #[error("shutdown signal handler stopped")]
    Shutdown(#[source] io::Error),
    #[error("delivery inbox lock is unavailable")]
    InboxLock,
    #[error("delivery worker panicked")]
    WorkerPanicked(#[source] tokio::task::JoinError),
    #[error("delivery worker stopped")]
    Worker(#[source] crate::DeliveryWorkerError),
}

impl SupervisionError {
    pub fn component(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Component(Box::new(error))
    }
}

/// Runs the HTTP listener and one required background component through drain.
///
/// # Errors
///
/// The endpoint, background component, or its stop boundary fails.
pub async fn supervise<C, S>(
    supervision: Supervision,
    component: C,
    shutdown: S,
    stop_component: impl FnOnce() -> Result<(), SupervisionError> + Send + 'static,
) -> Result<(), SupervisionError>
where
    C: Future<Output = Result<(), SupervisionError>>,
    S: Future<Output = io::Result<()>>,
{
    let Supervision {
        listener,
        router,
        ready,
        operations,
        endpoint,
        component: component_name,
    } = supervision;
    let server_stop = Arc::new(Notify::new());
    let observed_stop = Arc::clone(&server_stop);
    let server = serve(listener, router, async move {
        observed_stop.notified().await;
    });
    coordinate(
        RuntimeState {
            ready,
            operations,
            endpoint,
            component: component_name,
        },
        server,
        move || server_stop.notify_one(),
        component,
        shutdown,
        stop_component,
    )
    .await
}

struct RuntimeState {
    ready: Arc<AtomicBool>,
    operations: Operations,
    endpoint: EndpointDrain,
    component: ServiceComponent,
}

async fn coordinate<H, C, S>(
    state: RuntimeState,
    server: H,
    stop_server: impl FnOnce() + Send + 'static,
    component: C,
    shutdown: S,
    stop_component: impl FnOnce() -> Result<(), SupervisionError> + Send + 'static,
) -> Result<(), SupervisionError>
where
    H: Future<Output = io::Result<()>>,
    C: Future<Output = Result<(), SupervisionError>>,
    S: Future<Output = io::Result<()>>,
{
    let RuntimeState {
        ready,
        operations,
        endpoint,
        component: component_name,
    } = state;
    let mut server = Box::pin(server);
    let mut component = Box::pin(component);
    let mut shutdown = Box::pin(shutdown);
    let initial = tokio::select! {
        biased;
        result = &mut shutdown => Some((None, None, result)),
        result = &mut server => Some((Some(result), None, Ok(()))),
        result = &mut component => Some((None, Some(result), Ok(()))),
        () = std::future::ready(()) => None,
    };
    if initial.is_none() {
        ready.store(true, Ordering::Release);
        operations.emit(ServiceEvent::Ready);
    }
    let (early_server, early_component, shutdown_result) = if let Some(result) = initial {
        result
    } else {
        tokio::select! {
            result = &mut shutdown => (None, None, result),
            result = &mut server => (Some(result), None, Ok(())),
            result = &mut component => (None, Some(result), Ok(())),
        }
    };
    ready.store(false, Ordering::Release);
    let failed_early = early_component.is_some();
    if failed_early {
        operations.emit(ServiceEvent::Failed(component_name));
    }
    operations.emit(ServiceEvent::Draining);
    stop_server();
    let stop_component = tokio::task::spawn_blocking(stop_component);
    let stop_result = async {
        stop_component
            .await
            .map_err(SupervisionError::StopPanicked)
            .and_then(std::convert::identity)
    };
    let server_result = async {
        match early_server {
            Some(result) => Err(SupervisionError::HttpStopped(result.err())),
            None => server
                .await
                .map_err(|error| SupervisionError::HttpStopped(Some(error))),
        }
    };
    let component_result = async {
        match early_component {
            Some(result) => result.and(Err(SupervisionError::ComponentStopped)),
            None => component.await,
        }
    };
    let (server_result, stop_result, drain_result, component_result) = tokio::join!(
        server_result,
        stop_result,
        endpoint.wait(),
        component_result
    );
    if !failed_early && (stop_result.is_err() || component_result.is_err()) {
        operations.emit(ServiceEvent::Failed(component_name));
    }
    let result = server_result
        .and(
            drain_result
                .map(|_guard| ())
                .map_err(SupervisionError::EndpointDrain),
        )
        .and(shutdown_result.map_err(SupervisionError::Shutdown))
        .and(stop_result)
        .and(component_result);
    operations.emit(ServiceEvent::Stopped);
    result
}
