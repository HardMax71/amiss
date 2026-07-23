use std::convert::Infallible;
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use tokio::sync::Semaphore;
use tower_http::limit::RequestBodyLimitLayer;

use crate::DeliveryHeader;
use crate::receiver::headers;
use crate::request_body::{self, ReadError};

const HEALTH_PATH: &str = "/healthz";
const MAX_PATH_BYTES: usize = 1_024;
pub(crate) const MAX_CONCURRENT_EVALUATIONS: usize = 64;
pub(crate) const MAX_BODY_BYTES: usize = 8 * 1_024 * 1_024;
pub(crate) const MAX_HEADERS: u64 = 128;
pub(crate) const MAX_HEADER_BYTES: u64 = 32 * 1_024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationConfig {
    pub path: String,
    pub max_body_bytes: usize,
    pub max_headers: u64,
    pub max_header_bytes: u64,
    pub max_concurrent_evaluations: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EvaluationRequest<'a> {
    pub received_at_unix_millis: i64,
    pub headers: &'a [DeliveryHeader],
    pub body: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationConfigError;

impl fmt::Display for EvaluationConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("evaluation endpoint configuration is invalid")
    }
}

impl std::error::Error for EvaluationConfigError {}

type Evaluate = dyn for<'a> Fn(EvaluationRequest<'a>) -> StatusCode + Send + Sync + 'static;

#[derive(Clone)]
struct EvaluationState {
    evaluate: Arc<Evaluate>,
    max_body_bytes: usize,
    max_headers: u64,
    max_header_bytes: u64,
    permits: Arc<Semaphore>,
}

/// Builds one bounded synchronous endpoint for a provider-owned policy job.
///
/// # Errors
///
/// The path is not one exact static path or a limit is outside its hard bounds.
pub fn evaluation_router<F>(
    config: &EvaluationConfig,
    evaluate: F,
) -> Result<Router, EvaluationConfigError>
where
    F: for<'a> Fn(EvaluationRequest<'a>) -> StatusCode + Send + Sync + 'static,
{
    validate(config)?;
    let state = EvaluationState {
        evaluate: Arc::new(evaluate),
        max_body_bytes: config.max_body_bytes,
        max_headers: config.max_headers,
        max_header_bytes: config.max_header_bytes,
        permits: Arc::new(Semaphore::new(config.max_concurrent_evaluations)),
    };
    let evaluation =
        post(run).layer::<_, Infallible>(RequestBodyLimitLayer::new(config.max_body_bytes));
    Ok(Router::new()
        .route(&config.path, evaluation)
        .route(HEALTH_PATH, get(health))
        .with_state(state))
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn run(State(state): State<EvaluationState>, request: Request) -> StatusCode {
    let Some(received_at_unix_millis) = controller_time() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let (parts, body) = request.into_parts();
    if parts.uri.query().is_some() {
        return StatusCode::BAD_REQUEST;
    }
    if !headers::within_limits(&parts.headers, state.max_headers, state.max_header_bytes) {
        return StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE;
    }
    let Ok(permit) = Arc::clone(&state.permits).try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    let body = match request_body::read(body, state.max_body_bytes).await {
        Ok(body) => body,
        Err(ReadError::Invalid) => return StatusCode::PAYLOAD_TOO_LARGE,
        Err(ReadError::TimedOut) => return StatusCode::REQUEST_TIMEOUT,
    };
    let headers = headers::materialize(&parts.headers);
    let evaluate = Arc::clone(&state.evaluate);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        evaluate(EvaluationRequest {
            received_at_unix_millis,
            headers: &headers,
            body: body.as_ref(),
        })
    })
    .await
    .unwrap_or(StatusCode::SERVICE_UNAVAILABLE)
}

pub(crate) fn validate(config: &EvaluationConfig) -> Result<(), EvaluationConfigError> {
    let path = config.path.as_bytes();
    let exact_path = path.len() <= MAX_PATH_BYTES
        && path.first() == Some(&b'/')
        && config.path != "/"
        && config.path != HEALTH_PATH
        && path
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.'))
        && !config.path.contains("//");
    (exact_path
        && (1..=MAX_BODY_BYTES).contains(&config.max_body_bytes)
        && (1..=MAX_HEADERS).contains(&config.max_headers)
        && (1..=MAX_HEADER_BYTES).contains(&config.max_header_bytes)
        && (1..=MAX_CONCURRENT_EVALUATIONS).contains(&config.max_concurrent_evaluations))
    .then_some(())
    .ok_or(EvaluationConfigError)
}

fn controller_time() -> Option<i64> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(elapsed.as_millis()).ok()
}
