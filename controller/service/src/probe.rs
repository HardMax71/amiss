use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::extract::Extension;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::get;
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};

use crate::Operations;

pub(crate) const HEALTH_PATH: &str = "/healthz";
pub(crate) const METRICS_PATH: &str = "/metrics";
pub(crate) const READY_PATH: &str = "/readyz";

/// Barrier for work accepted by one provider endpoint.
pub struct EndpointDrain {
    permits: Arc<Semaphore>,
    count: u32,
}

impl EndpointDrain {
    /// Waits for every accepted request and holds endpoint capacity.
    ///
    /// # Errors
    ///
    /// The private endpoint semaphore was closed.
    pub async fn wait(self) -> Result<OwnedSemaphorePermit, AcquireError> {
        self.permits.acquire_many_owned(self.count).await
    }
}

pub(crate) fn work_permits(count: usize) -> Option<(Arc<Semaphore>, EndpointDrain)> {
    let drain_count = u32::try_from(count).ok()?;
    let permits = Arc::new(Semaphore::new(count));
    Some((
        Arc::clone(&permits),
        EndpointDrain {
            permits,
            count: drain_count,
        },
    ))
}

pub(crate) fn routes<S>(
    router: Router<S>,
    ready: Arc<AtomicBool>,
    operations: Operations,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router
        .route(HEALTH_PATH, get(health))
        .route(READY_PATH, get(readiness))
        .route(METRICS_PATH, get(metrics))
        .layer(Extension(ready))
        .layer(Extension(operations))
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn readiness(Extension(ready): Extension<Arc<AtomicBool>>) -> StatusCode {
    if ready.load(Ordering::Acquire) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn metrics(Extension(operations): Extension<Operations>) -> Response {
    let mut body = String::new();
    if operations.encode(&mut body).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    (
        [(
            header::CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )],
        body,
    )
        .into_response()
}
