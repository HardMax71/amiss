mod tests;

use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use amiss_controller::{
    AcquiringRunner, AdapterRegistry, Controller, ControllerClock, DeliveryHeader, DeliveryRoute,
    FileLedgerRoot, IngressPolicy, PlanRegistry, ProviderAdapter, SystemClock, UntrustedDelivery,
    register_plan,
};
use amiss_controller_git::GitFetchBounds;
use amiss_controller_gitlab::{GitLabMergeTrainAdapter, policy_job_accepted};
use amiss_controller_service::{
    AdmissionRejection, EndpointDrain, EvaluationRequest, Operations, ServiceComponent,
    Supervision, SupervisionError, check_lane, evaluation_router_with_clock, shutdown_signal,
    supervise,
};
use axum::Router;
use axum::http::StatusCode;
use secrecy::{ExposeSecret as _, SecretString};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::{Instant, MissedTickBehavior};

use crate::acquisition::gitlab_acquisition;
use crate::config::ServiceConfig;

const LEDGER_MAINTENANCE_INTERVAL: Duration = Duration::from_mins(1);

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
    ledger: Arc<FileLedgerRoot>,
    ready: Arc<AtomicBool>,
    operations: Operations,
    endpoint: EndpointDrain,
}

struct Lane {
    route: DeliveryRoute,
    adapter: Arc<dyn ProviderAdapter>,
    plans: PlanRegistry,
    ledger: Arc<FileLedgerRoot>,
    clock: Arc<dyn ControllerClock>,
    ingress: IngressPolicy,
    project_id: u64,
    git_username: String,
    git_token: SecretString,
    git_bounds: GitFetchBounds,
    bootstrap: PathBuf,
    scratch: PathBuf,
    bootstrap_timeout: Duration,
    statement_validity: Duration,
    operations: Operations,
}

/// Runs one synchronous GitLab policy-job lane until shutdown.
///
/// # Errors
///
/// Returns an error when the lane cannot start or continue safely.
pub async fn run(config: ServiceConfig) -> Result<(), ServiceError> {
    let shutdown = shutdown_signal()
        .map_err(|_defect| ServiceError("shutdown signal handler cannot be installed"))?;
    let Prepared {
        listen,
        router,
        ledger,
        ready,
        operations,
        endpoint,
    } = prepare(config)?;
    let listener = TcpListener::bind(listen)
        .await
        .map_err(|_defect| ServiceError("HTTP listener cannot bind"))?;
    let maintenance_stop = Arc::new(Notify::new());
    let component_stop = Arc::clone(&maintenance_stop);
    let component = maintain_ledger(ledger, Arc::clone(&maintenance_stop), operations.clone());
    let component = async move { component.await.map_err(|error| SupervisionError(error.0)) };
    supervise(
        Supervision {
            listener,
            router,
            ready,
            operations,
            endpoint,
            component: ServiceComponent::Maintenance,
        },
        component,
        shutdown,
        move || {
            component_stop.notify_one();
            Ok(())
        },
    )
    .await
    .map_err(|error| ServiceError(error.0))
}

fn prepare(config: ServiceConfig) -> Result<Prepared, ServiceError> {
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let ledger = Arc::new(
        FileLedgerRoot::open_with_clock(&config.ledger_root, config.ledger, Arc::clone(&clock))
            .map_err(|_defect| ServiceError("delivery record cannot be opened"))?,
    );
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, config.scope, Arc::clone(&config.plan))
        .map_err(|_defect| ServiceError("check plan cannot be registered"))?;
    let adapter: Arc<dyn ProviderAdapter> =
        Arc::new(GitLabMergeTrainAdapter::new(config.source, config.client));
    let operations = Operations::default();
    let lane = Arc::new(Lane {
        route: config.route,
        adapter,
        plans,
        ledger: Arc::clone(&ledger),
        clock,
        ingress: config.ingress,
        project_id: config.project_id,
        git_username: config.git_username,
        git_token: config.git_token,
        git_bounds: config.git_bounds,
        bootstrap: config.bootstrap,
        scratch: config.scratch,
        bootstrap_timeout: config.bootstrap_timeout,
        statement_validity: config.statement_validity,
        operations: operations.clone(),
    });
    let evaluation = config.evaluation;
    let ready = Arc::new(AtomicBool::new(false));
    let (router, endpoint) = evaluation_router_with_clock(
        &evaluation,
        Arc::clone(&ready),
        operations.clone(),
        Arc::clone(&lane.clock),
        move |request| evaluate(&lane, request),
    )
    .map_err(|_defect| ServiceError("HTTP evaluation configuration is invalid"))?;
    Ok(Prepared {
        listen: config.listen,
        router,
        ledger,
        ready,
        operations,
        endpoint,
    })
}

async fn maintain_ledger(
    ledger: Arc<FileLedgerRoot>,
    stop: Arc<Notify>,
    operations: Operations,
) -> Result<(), ServiceError> {
    maintenance_loop(LEDGER_MAINTENANCE_INTERVAL, stop, move || {
        cleanup_ledger(Arc::clone(&ledger), operations.clone())
    })
    .await
}

async fn maintenance_loop<F, M>(
    period: Duration,
    stop: Arc<Notify>,
    mut maintenance: F,
) -> Result<(), ServiceError>
where
    F: FnMut() -> M,
    M: Future<Output = Result<(), ServiceError>>,
{
    let start = Instant::now()
        .checked_add(period)
        .ok_or(ServiceError("maintenance interval overflow"))?;
    let mut ticks = tokio::time::interval_at(start, period);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            () = stop.notified() => return Ok(()),
            _instant = ticks.tick() => maintenance().await?,
        }
    }
}

async fn cleanup_ledger(
    ledger: Arc<FileLedgerRoot>,
    operations: Operations,
) -> Result<(), ServiceError> {
    let removed = tokio::task::spawn_blocking(move || ledger.cleanup())
        .await
        .map_err(|_panic| ServiceError("delivery record maintenance panicked"))?
        .map_err(|_defect| ServiceError("delivery record maintenance failed"))?;
    operations.maintenance_runs.inc();
    operations.maintenance_removals.inc_by(
        removed
            .removed_records
            .saturating_add(removed.removed_reports)
            .saturating_add(removed.removed_temporary),
    );
    Ok(())
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
    match check_lane(
        &lane.ingress,
        &lane.plans,
        untrusted,
        lane.clock.as_ref(),
        |checked| {
            lane.adapter
                .authenticate(checked)
                .map_err(|_defect| AdmissionRejection::Unauthorized)
        },
    ) {
        Ok(_accepted) => result_status(handle(lane, untrusted)),
        Err(rejection) => rejection_status(rejection),
    }
}

fn handle(
    lane: &Lane,
    untrusted: UntrustedDelivery<'_>,
) -> Result<amiss_controller::HandleOutcome, ServiceError> {
    let clock = Arc::clone(&lane.clock);
    let ledger = lane
        .ledger
        .session()
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
    )
    .with_external_sink(Arc::new(lane.operations.clone()));
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
