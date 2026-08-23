mod tests;

use std::fmt;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::registry::Registry;
use serde::Serialize;

const EVENT_SCHEMA: &str = "amiss/controller-event/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceComponent {
    Worker,
    Maintenance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceEvent {
    Ready,
    Draining,
    Stopped,
    Failed(ServiceComponent),
}

#[derive(Clone)]
pub struct Operations {
    registry: Arc<Registry>,
    events: Arc<dyn Fn(ServiceEvent) + Send + Sync>,
    pub provider_requests: Counter,
    pub provider_acceptances: Counter,
    pub provider_refusals: Counter,
    pub provider_unavailable: Counter,
    pub delivery_attempts: Counter,
    pub delivery_completions: Counter,
    pub delivery_retries: Counter,
    pub delivery_discards: Counter,
    pub maintenance_runs: Counter,
    pub maintenance_removals: Counter,
    pub external_refuted: Counter,
    pub external_unproven: Counter,
    pub external_reachable: Counter,
    pub external_incomplete: Counter,
}

impl Operations {
    pub fn with_event_sink(events: impl Fn(ServiceEvent) + Send + Sync + 'static) -> Self {
        let mut registry = Registry::with_prefix("amiss_controller");
        let provider_requests = counter(
            &mut registry,
            "provider_requests",
            "Provider requests answered by this process.",
        );
        let provider_acceptances = counter(
            &mut registry,
            "provider_acceptances",
            "Provider requests answered with a successful status.",
        );
        let provider_refusals = counter(
            &mut registry,
            "provider_refusals",
            "Provider requests answered with a client-error status.",
        );
        let provider_unavailable = counter(
            &mut registry,
            "provider_unavailable",
            "Provider requests answered with any other non-success status.",
        );
        let delivery_attempts = counter(
            &mut registry,
            "delivery_attempts",
            "Durable webhook deliveries claimed for processing.",
        );
        let delivery_completions = counter(
            &mut registry,
            "delivery_completions",
            "Durable webhook deliveries removed after processing.",
        );
        let delivery_retries = counter(
            &mut registry,
            "delivery_retries",
            "Durable webhook deliveries rescheduled after processing.",
        );
        let delivery_discards = counter(
            &mut registry,
            "delivery_discards",
            "Durable webhook deliveries removed after failed reauthentication.",
        );
        let maintenance_runs = counter(
            &mut registry,
            "maintenance_runs",
            "Successful durable-state maintenance runs.",
        );
        let maintenance_removals = counter(
            &mut registry,
            "maintenance_removals",
            "Durable-state entries removed by maintenance.",
        );
        let external_refuted = counter(
            &mut registry,
            "external_refuted",
            "External destinations a retained assessment refuted.",
        );
        let external_unproven = counter(
            &mut registry,
            "external_unproven",
            "External destinations a retained assessment left unproven.",
        );
        let external_reachable = counter(
            &mut registry,
            "external_reachable",
            "External destinations a retained assessment found reachable.",
        );
        let external_incomplete = counter(
            &mut registry,
            "external_incomplete",
            "External verifications that could not finish.",
        );
        Self {
            registry: Arc::new(registry),
            events: Arc::new(events),
            provider_requests,
            provider_acceptances,
            provider_refusals,
            provider_unavailable,
            delivery_attempts,
            delivery_completions,
            delivery_retries,
            delivery_discards,
            maintenance_runs,
            maintenance_removals,
            external_refuted,
            external_unproven,
            external_reachable,
            external_incomplete,
        }
    }

    fn record_response(&self, status: StatusCode) {
        self.provider_requests.inc();
        if status.is_success() {
            self.provider_acceptances.inc();
        } else if status.is_client_error() {
            self.provider_refusals.inc();
        } else {
            self.provider_unavailable.inc();
        }
    }

    pub fn emit(&self, event: ServiceEvent) {
        (self.events)(event);
    }

    pub(crate) fn encode(&self, output: &mut String) -> fmt::Result {
        encode(output, &self.registry)
    }
}

pub(crate) async fn record_provider_response(
    State((operations, ready)): State<(Operations, Arc<AtomicBool>)>,
    request: Request,
    next: Next,
) -> Response {
    let provider_request = request.method() == Method::POST;
    let response = if provider_request && !ready.load(Ordering::Acquire) {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    } else {
        next.run(request).await
    };
    if provider_request {
        operations.record_response(response.status());
    }
    response
}

impl Default for Operations {
    fn default() -> Self {
        Self::with_event_sink(write_event)
    }
}

#[derive(Serialize)]
struct EventLine {
    schema: &'static str,
    level: &'static str,
    event: &'static str,
    component: &'static str,
}

impl From<ServiceEvent> for EventLine {
    fn from(event: ServiceEvent) -> Self {
        let (level, event, component) = match event {
            ServiceEvent::Ready => ("info", "ready", "service"),
            ServiceEvent::Draining => ("info", "draining", "service"),
            ServiceEvent::Stopped => ("info", "stopped", "service"),
            ServiceEvent::Failed(ServiceComponent::Worker) => ("error", "failed", "worker"),
            ServiceEvent::Failed(ServiceComponent::Maintenance) => {
                ("error", "failed", "maintenance")
            }
        };
        Self {
            schema: EVENT_SCHEMA,
            level,
            event,
            component,
        }
    }
}

fn write_event(event: ServiceEvent) {
    let _ignored = write_event_to(&mut io::stderr().lock(), event);
}

fn write_event_to(output: &mut impl io::Write, event: ServiceEvent) -> io::Result<()> {
    let mut line = serde_json::to_vec(&EventLine::from(event)).map_err(io::Error::other)?;
    line.push(b'\n');
    output.write_all(&line)
}

/// One registered label-free counter; the name gains the registry prefix.
fn counter(registry: &mut Registry, name: &str, help: &str) -> Counter {
    let counter = Counter::default();
    registry.register(name, help, counter.clone());
    counter
}

/// The counter sink the queued lanes hand the controller.
impl amiss_controller::ExternalSink for Operations {
    fn assessed(&self, tally: &amiss_controller::ExternalTally) {
        self.external_refuted.inc_by(tally.refuted);
        self.external_unproven.inc_by(tally.unproven);
        self.external_reachable.inc_by(tally.reachable);
    }

    fn incomplete(&self) {
        self.external_incomplete.inc();
    }
}
