mod body;
mod headers;

use std::sync::Arc;

use amiss_controller::ControllerClock;
use axum::extract::Request;
use axum::http::StatusCode;
use tokio::sync::Semaphore;

use crate::DeliveryHeader;
use crate::probe::{EndpointDrain, HEALTH_PATH, METRICS_PATH, READY_PATH, work_permits};

use self::body::ReadError;

const MAX_PATH_BYTES: usize = 1_024;
pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 64;
pub(crate) const MAX_BODY_BYTES: usize = 8 * 1_024 * 1_024;
pub(crate) const MAX_HEADERS: u64 = 128;
pub(crate) const MAX_HEADER_BYTES: u64 = 32 * 1_024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointConfig {
    pub path: String,
    pub max_body_bytes: usize,
    pub max_headers: u64,
    pub max_header_bytes: u64,
    pub max_concurrent_requests: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EndpointConfigError {
    #[error("endpoint path is not one exact static path")]
    Path,
    #[error("endpoint limits are invalid")]
    Limits,
}

#[derive(Clone)]
pub(crate) struct EndpointState {
    max_body_bytes: usize,
    max_headers: u64,
    max_header_bytes: u64,
    permits: Arc<Semaphore>,
    clock: Arc<dyn ControllerClock>,
}

pub(crate) fn prepare(
    config: &EndpointConfig,
    clock: Arc<dyn ControllerClock>,
) -> Result<(EndpointState, EndpointDrain), EndpointConfigError> {
    validate(config)?;
    let (permits, drain) =
        work_permits(config.max_concurrent_requests).ok_or(EndpointConfigError::Limits)?;
    Ok((
        EndpointState {
            max_body_bytes: config.max_body_bytes,
            max_headers: config.max_headers,
            max_header_bytes: config.max_header_bytes,
            permits,
            clock,
        },
        drain,
    ))
}

pub(crate) async fn bounded_request<T>(
    state: &EndpointState,
    request: Request,
    handle: impl FnOnce(i64, &[DeliveryHeader], &[u8]) -> T + Send + 'static,
) -> Result<T, StatusCode>
where
    T: Send + 'static,
{
    let received_at_unix_millis = state
        .clock
        .now_unix_millis()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let (parts, body) = request.into_parts();
    parts
        .uri
        .query()
        .is_none()
        .then_some(())
        .ok_or(StatusCode::BAD_REQUEST)?;
    headers::within_limits(&parts.headers, state.max_headers, state.max_header_bytes)
        .then_some(())
        .ok_or(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE)?;
    let permit = Arc::clone(&state.permits)
        .try_acquire_owned()
        .map_err(|_closed| StatusCode::SERVICE_UNAVAILABLE)?;
    let body = body::read(body, state.max_body_bytes)
        .await
        .map_err(|defect| match defect {
            ReadError::Invalid => StatusCode::PAYLOAD_TOO_LARGE,
            ReadError::TimedOut => StatusCode::REQUEST_TIMEOUT,
        })?;
    let headers = headers::materialize(&parts.headers);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        handle(received_at_unix_millis, &headers, body.as_ref())
    })
    .await
    .map_err(|_panic| StatusCode::SERVICE_UNAVAILABLE)
}

pub(crate) fn validate(config: &EndpointConfig) -> Result<(), EndpointConfigError> {
    ((1..=MAX_BODY_BYTES).contains(&config.max_body_bytes)
        && (1..=MAX_HEADERS).contains(&config.max_headers)
        && (1..=MAX_HEADER_BYTES).contains(&config.max_header_bytes)
        && (1..=MAX_CONCURRENT_REQUESTS).contains(&config.max_concurrent_requests))
    .then_some(())
    .ok_or(EndpointConfigError::Limits)?;
    let path = config.path.as_bytes();
    let exact = path.len() <= MAX_PATH_BYTES
        && path.first() == Some(&b'/')
        && config.path != "/"
        && config.path != HEALTH_PATH
        && config.path != METRICS_PATH
        && config.path != READY_PATH
        && path
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.'))
        && !config.path.contains("//");
    exact.then_some(()).ok_or(EndpointConfigError::Path)
}
