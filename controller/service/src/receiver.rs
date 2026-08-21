mod admission;
mod handler;

use std::convert::Infallible;
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
use crate::endpoint::{self, EndpointConfig, EndpointConfigError};
use crate::operations::record_provider_response;
use crate::probe::EndpointDrain;

/// Builds a provider-neutral receiver around one open durable inbox, stamping
/// each arrival from the system clock.
///
/// # Errors
///
/// Returns an error when the delivery path is not an exact static path or a
/// receiver limit is zero or outside its hard ceiling.
pub fn router(
    config: &EndpointConfig,
    inbox: Arc<Mutex<Inbox>>,
    admission: Arc<dyn DeliveryAdmission>,
    ready: Arc<AtomicBool>,
    operations: Operations,
) -> Result<(Router, EndpointDrain), EndpointConfigError> {
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
    config: &EndpointConfig,
    inbox: Arc<Mutex<Inbox>>,
    admission: Arc<dyn DeliveryAdmission>,
    ready: Arc<AtomicBool>,
    operations: Operations,
    clock: Arc<dyn ControllerClock>,
) -> Result<(Router, EndpointDrain), EndpointConfigError> {
    let (endpoint, drain) = endpoint::prepare(config, clock)?;
    let state = ReceiverState {
        admission,
        inbox,
        endpoint,
    };
    let delivery = post(receive)
        .layer::<_, Infallible>(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(middleware::from_fn_with_state(
            (operations.clone(), Arc::clone(&ready)),
            record_provider_response,
        ));
    Ok((
        crate::probe::routes(
            Router::new().route(&config.path, delivery),
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
