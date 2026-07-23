mod admission;
mod handler;
pub(crate) mod headers;

use std::convert::Infallible;
use std::fmt;
use std::io;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::routing::{get, post};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tower_http::limit::RequestBodyLimitLayer;

pub use self::admission::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission,
};
use self::handler::{ReceiverState, health, receive};
use crate::Inbox;
use crate::evaluation::{MAX_BODY_BYTES, MAX_HEADER_BYTES, MAX_HEADERS};

const HEALTH_PATH: &str = "/healthz";
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

/// Builds a provider-neutral receiver around one open durable inbox.
///
/// # Errors
///
/// Returns an error when the delivery path is not an exact static path or a
/// receiver limit is zero or outside its hard ceiling.
pub fn router(
    config: &ReceiverConfig,
    inbox: Arc<Mutex<Inbox>>,
    admission: Arc<dyn DeliveryAdmission>,
) -> Result<Router, ReceiverConfigError> {
    validate(config)?;
    let delivery_path = config.delivery_path.clone();
    let state = ReceiverState {
        admission,
        inbox,
        max_body_bytes: config.max_body_bytes,
        max_headers: config.max_headers,
        max_header_bytes: config.max_header_bytes,
        permits: Arc::new(Semaphore::new(config.max_concurrent_deliveries)),
    };
    let delivery =
        post(receive).layer::<_, Infallible>(RequestBodyLimitLayer::new(config.max_body_bytes));
    Ok(Router::new()
        .route(&delivery_path, delivery)
        .route(HEALTH_PATH, get(health))
        .with_state(state))
}

/// Serves a receiver on an already-bound TCP listener.
///
/// # Errors
///
/// Returns the listener or connection error reported by Axum.
pub async fn serve(listener: TcpListener, router: Router) -> io::Result<()> {
    axum::serve(listener, router).await
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
        && path
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.'))
        && !config.delivery_path.contains("//");
    exact.then_some(()).ok_or(ReceiverConfigError::Path)
}
