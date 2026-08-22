use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use amiss_controller::{ControllerClock, SystemClock};
use axum::Router;
use axum::extract::{Request, State};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use tower_http::limit::RequestBodyLimitLayer;

use crate::endpoint::{self, EndpointConfig, EndpointConfigError, EndpointState};
use crate::operations::record_provider_response;
use crate::probe::EndpointDrain;
use crate::{DeliveryHeader, Operations};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EvaluationRequest<'a> {
    pub received_at_unix_millis: i64,
    pub headers: &'a [DeliveryHeader],
    pub body: &'a [u8],
}

type Evaluate = dyn for<'a> Fn(EvaluationRequest<'a>) -> Response + Send + Sync + 'static;

#[derive(Clone)]
struct EvaluationState {
    evaluate: Arc<Evaluate>,
    endpoint: EndpointState,
}

/// Builds one bounded synchronous endpoint for a provider-owned policy job,
/// reading the system clock.
///
/// # Errors
///
/// The path is not one exact static path or a limit is outside its hard bounds.
pub fn evaluation_router<F, O>(
    config: &EndpointConfig,
    ready: Arc<AtomicBool>,
    operations: Operations,
    evaluate: F,
) -> Result<(Router, EndpointDrain), EndpointConfigError>
where
    F: for<'a> Fn(EvaluationRequest<'a>) -> O + Send + Sync + 'static,
    O: IntoResponse + 'static,
{
    evaluation_router_with_clock(config, ready, operations, Arc::new(SystemClock), evaluate)
}

/// Builds one bounded synchronous endpoint for a provider-owned policy job,
/// reading the clock supplied.
///
/// # Errors
///
/// The path is not one exact static path or a limit is outside its hard bounds.
pub fn evaluation_router_with_clock<F, O>(
    config: &EndpointConfig,
    ready: Arc<AtomicBool>,
    operations: Operations,
    clock: Arc<dyn ControllerClock>,
    evaluate: F,
) -> Result<(Router, EndpointDrain), EndpointConfigError>
where
    F: for<'a> Fn(EvaluationRequest<'a>) -> O + Send + Sync + 'static,
    O: IntoResponse + 'static,
{
    let (endpoint, drain) = endpoint::prepare(config, clock)?;
    let state = EvaluationState {
        evaluate: Arc::new(move |request| evaluate(request).into_response()),
        endpoint,
    };
    let evaluation = post(run)
        .layer::<_, Infallible>(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(middleware::from_fn_with_state(
            (operations.clone(), Arc::clone(&ready)),
            record_provider_response,
        ));
    Ok((
        crate::probe::routes(
            Router::new().route(&config.path, evaluation),
            ready,
            operations,
        )
        .with_state(state),
        drain,
    ))
}

async fn run(State(state): State<EvaluationState>, request: Request) -> Response {
    let evaluate = Arc::clone(&state.evaluate);
    endpoint::bounded_request(
        &state.endpoint,
        request,
        move |received_at_unix_millis, headers, body| {
            evaluate(EvaluationRequest {
                received_at_unix_millis,
                headers,
                body,
            })
        },
    )
    .await
    .unwrap_or_else(IntoResponse::into_response)
}
