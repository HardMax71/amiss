use std::fmt;
use std::future::IntoFuture as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    AcquiringRunner, AdapterRegistry, Controller, ControllerClock, DeliveryHeader, DeliveryRoute,
    FileLedger, FileLedgerConfig, IngressPolicy, PlanRegistry, ProviderAdapter, SystemClock,
    UntrustedDelivery, register_plan,
};
use amiss_controller_git::GitFetchBounds;
use amiss_controller_gitlab::{GitLabMergeTrainAdapter, policy_job_accepted};
use amiss_controller_service::{
    AdmissionRejection, EvaluationRequest, check_lane, evaluation_router, shutdown_signal,
};
use axum::Router;
use axum::http::StatusCode;
use secrecy::{ExposeSecret as _, SecretString};
use tokio::net::TcpListener;

use crate::acquisition::gitlab_acquisition;
use crate::config::ServiceConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceError(pub &'static str);

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ServiceError {}

struct Prepared {
    listen: std::net::SocketAddr,
    router: Router,
}

struct Lane {
    route: DeliveryRoute,
    adapter: Arc<dyn ProviderAdapter>,
    plans: PlanRegistry,
    ledger: FileLedgerConfig,
    ledger_root: PathBuf,
    ingress: IngressPolicy,
    project_id: u64,
    git_username: String,
    git_token: SecretString,
    git_bounds: GitFetchBounds,
    bootstrap: PathBuf,
    scratch: PathBuf,
    bootstrap_timeout: Duration,
    statement_validity: Duration,
}

/// Runs one synchronous GitLab policy-job lane until shutdown.
///
/// # Errors
///
/// A trust input, record root, endpoint, listener, or shutdown signal is invalid.
pub async fn run(config: ServiceConfig) -> Result<(), ServiceError> {
    let prepared = prepare(config)?;
    let listener = TcpListener::bind(prepared.listen)
        .await
        .map_err(|_defect| ServiceError("HTTP listener cannot bind"))?;
    let mut server = Box::pin(axum::serve(listener, prepared.router).into_future());
    tokio::select! {
        result = &mut server => {
            result.map_err(|_defect| ServiceError("HTTP evaluation service stopped"))
        }
        signal = shutdown_signal() => {
            signal.map_err(|_defect| ServiceError("shutdown signal cannot be observed"))
        }
    }
}

fn prepare(config: ServiceConfig) -> Result<Prepared, ServiceError> {
    FileLedger::open(&config.ledger_root, config.ledger)
        .map_err(|_defect| ServiceError("delivery record cannot be opened"))?;
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, config.scope, Arc::clone(&config.plan))
        .map_err(|_defect| ServiceError("check plan cannot be registered"))?;
    let adapter: Arc<dyn ProviderAdapter> =
        Arc::new(GitLabMergeTrainAdapter::new(config.source, config.client));
    let lane = Arc::new(Lane {
        route: config.route,
        adapter,
        plans,
        ledger: config.ledger,
        ledger_root: config.ledger_root,
        ingress: config.ingress,
        project_id: config.project_id,
        git_username: config.git_username,
        git_token: config.git_token,
        git_bounds: config.git_bounds,
        bootstrap: config.bootstrap,
        scratch: config.scratch,
        bootstrap_timeout: config.bootstrap_timeout,
        statement_validity: config.statement_validity,
    });
    let evaluation = config.evaluation;
    let router = evaluation_router(&evaluation, move |request| evaluate(&lane, request))
        .map_err(|_defect| ServiceError("HTTP evaluation configuration is invalid"))?;
    Ok(Prepared {
        listen: config.listen,
        router,
    })
}

fn evaluate(lane: &Lane, request: EvaluationRequest<'_>) -> StatusCode {
    let headers = request
        .headers
        .iter()
        .map(|header| DeliveryHeader {
            name: &header.name,
            value: &header.value,
        })
        .collect::<Vec<_>>();
    let untrusted = UntrustedDelivery {
        route: &lane.route,
        received_at_unix_millis: request.received_at_unix_millis,
        headers: &headers,
        body: request.body,
    };
    match check_lane(&lane.ingress, &lane.plans, untrusted, |checked| {
        lane.adapter
            .authenticate(checked)
            .map_err(|_defect| AdmissionRejection::Unauthorized)
    }) {
        Ok(_accepted) => result_status(handle(lane, untrusted)),
        Err(rejection) => rejection_status(rejection),
    }
}

fn handle(
    lane: &Lane,
    untrusted: UntrustedDelivery<'_>,
) -> Result<amiss_controller::HandleOutcome, ServiceError> {
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let ledger = FileLedger::open_with_clock(&lane.ledger_root, lane.ledger, Arc::clone(&clock))
        .map_err(|_defect| ServiceError("evaluation unavailable"))?;
    let acquisition = gitlab_acquisition(
        lane.git_bounds,
        lane.project_id,
        lane.git_username.clone(),
        clone_secret(&lane.git_token),
    );
    let runner = AcquiringRunner::new(
        acquisition,
        lane.bootstrap.clone(),
        lane.scratch.clone(),
        lane.bootstrap_timeout,
        lane.statement_validity,
        Arc::clone(&clock),
    )
    .ok_or(ServiceError("evaluation unavailable"))?;
    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::clone(&lane.adapter))
        .map_err(|_defect| ServiceError("evaluation unavailable"))?;
    let mut controller = Controller::new_with_clock(
        registry,
        lane.plans.clone(),
        ledger,
        runner,
        lane.ingress,
        clock,
    );
    controller
        .handle(untrusted)
        .map_err(|_defect| ServiceError("evaluation unavailable"))
}

fn result_status<E>(result: Result<amiss_controller::HandleOutcome, E>) -> StatusCode {
    match result {
        Ok(outcome) if policy_job_accepted(&outcome) => StatusCode::NO_CONTENT,
        Ok(_) => StatusCode::PRECONDITION_FAILED,
        Err(_defect) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

const fn rejection_status(rejection: AdmissionRejection) -> StatusCode {
    match rejection {
        AdmissionRejection::Malformed => StatusCode::BAD_REQUEST,
        AdmissionRejection::Unauthorized => StatusCode::UNAUTHORIZED,
        AdmissionRejection::Forbidden => StatusCode::FORBIDDEN,
    }
}

fn clone_secret(secret: &SecretString) -> SecretString {
    SecretString::from(secret.expose_secret().to_owned())
}

#[path = "../tests/internal/runtime.rs"]
mod tests;
