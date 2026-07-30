mod admission;
mod handler;
pub(crate) mod headers;

use std::convert::Infallible;
use std::fmt;
use std::future::{Future, IntoFuture as _};
use std::io;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use amiss_controller::{ControllerClock, SystemClock};
use axum::Router;
use axum::middleware;
use axum::routing::post;
use tokio::net::TcpListener;
use tower_http::limit::RequestBodyLimitLayer;

pub use self::admission::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission,
};
use self::handler::{ReceiverState, receive};
use crate::Inbox;
use crate::Operations;
use crate::evaluation::{MAX_BODY_BYTES, MAX_HEADER_BYTES, MAX_HEADERS};
use crate::operations::record_provider_response;
use crate::probe::{EndpointDrain, HEALTH_PATH, METRICS_PATH, READY_PATH, work_permits};

const MAX_PATH_BYTES: usize = 1_024;
pub(crate) const MAX_CONCURRENT_DELIVERIES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiverConfig {
    pub delivery_path: String,
    pub max_body_bytes: usize,
    pub max_headers: u64,
    pub max_header_bytes: u64,
    pub max_concurrent_deliveries: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiverConfigError {
    Path,
    Limits,
}

impl fmt::Display for ReceiverConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path => formatter.write_str("receiver path is not one exact static path"),
            Self::Limits => formatter.write_str("receiver limits are invalid"),
        }
    }
}

impl std::error::Error for ReceiverConfigError {}

/// Builds a provider-neutral receiver around one open durable inbox, stamping
/// each arrival from the system clock.
///
/// # Errors
///
/// Returns an error when the delivery path is not an exact static path or a
/// receiver limit is zero or outside its hard ceiling.
pub fn router(
    config: &ReceiverConfig,
    inbox: Arc<Mutex<Inbox>>,
    admission: Arc<dyn DeliveryAdmission>,
    ready: Arc<AtomicBool>,
    operations: Operations,
) -> Result<(Router, EndpointDrain), ReceiverConfigError> {
    router_with_clock(
        config,
        inbox,
        admission,
        ready,
        operations,
        Arc::new(SystemClock),
    )
}

/// Builds a provider-neutral receiver around one open durable inbox, stamping
/// each arrival from the clock supplied.
///
/// # Errors
///
/// Returns an error when the delivery path is not an exact static path or a
/// receiver limit is zero or outside its hard ceiling.
pub fn router_with_clock(
    config: &ReceiverConfig,
    inbox: Arc<Mutex<Inbox>>,
    admission: Arc<dyn DeliveryAdmission>,
    ready: Arc<AtomicBool>,
    operations: Operations,
    clock: Arc<dyn ControllerClock>,
) -> Result<(Router, EndpointDrain), ReceiverConfigError> {
    validate(config)?;
    let (permits, drain) =
        work_permits(config.max_concurrent_deliveries).ok_or(ReceiverConfigError::Limits)?;
    let delivery_path = config.delivery_path.clone();
    let state = ReceiverState {
        admission,
        inbox,
        max_body_bytes: config.max_body_bytes,
        max_headers: config.max_headers,
        max_header_bytes: config.max_header_bytes,
        permits,
        clock,
    };
    let delivery = post(receive)
        .layer::<_, Infallible>(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(middleware::from_fn_with_state(
            (operations.clone(), Arc::clone(&ready)),
            record_provider_response,
        ));
    Ok((
        crate::probe::routes(
            Router::new().route(&delivery_path, delivery),
            ready,
            operations,
        )
        .with_state(state),
        drain,
    ))
}

/// Serves a receiver on an already-bound TCP listener until graceful shutdown.
///
/// # Errors
///
/// Returns the listener or connection error reported by Axum.
pub async fn serve<F>(listener: TcpListener, router: Router, shutdown: F) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .into_future()
        .await
}

pub(crate) fn validate(config: &ReceiverConfig) -> Result<(), ReceiverConfigError> {
    if !(1..=MAX_BODY_BYTES).contains(&config.max_body_bytes)
        || !(1..=MAX_HEADERS).contains(&config.max_headers)
        || !(1..=MAX_HEADER_BYTES).contains(&config.max_header_bytes)
        || !(1..=MAX_CONCURRENT_DELIVERIES).contains(&config.max_concurrent_deliveries)
    {
        return Err(ReceiverConfigError::Limits);
    }
    let path = config.delivery_path.as_bytes();
    let exact = path.len() <= MAX_PATH_BYTES
        && path.first() == Some(&b'/')
        && config.delivery_path != "/"
        && config.delivery_path != HEALTH_PATH
        && config.delivery_path != METRICS_PATH
        && config.delivery_path != READY_PATH
        && path
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.'))
        && !config.delivery_path.contains("//");
    exact.then_some(()).ok_or(ReceiverConfigError::Path)
}
