mod tests;

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use amiss_controller::{
    AcquiringRunner, AdapterRegistry, ArtifactReference, Controller, ControllerClock,
    ControllerError, DeliveryHeader, DeliveryRoute, FileLedgerError, FileLedgerRoot, IngressPolicy,
    PlanError, PlanRegistry, ProviderAdapter, RegistryError, SystemClock, UntrustedDelivery,
    register_plan,
};
use amiss_controller_git::GitFetchBounds;
use amiss_controller_gitlab::{GitLabMergeTrainAdapter, policy_job_accepted};
use amiss_controller_service::{
    AdmissionRejection, EndpointConfigError, EndpointDrain, EvaluationRequest, Operations,
    ServiceComponent, Supervision, SupervisionError, artifact_routes, check_lane,
    evaluation_router_with_clock, open_artifact_service, shutdown_signal, supervise,
};
use axum::Router;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};
use secrecy::{ExposeSecret as _, SecretString};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::{Instant, MissedTickBehavior};

use crate::acquisition::gitlab_acquisition;
use crate::config::ServiceConfig;

const LEDGER_MAINTENANCE_INTERVAL: Duration = Duration::from_mins(1);

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("shutdown signal handler cannot be installed")]
    ShutdownInstall(#[source] std::io::Error),
    #[error("HTTP listener cannot bind")]
    Listener(#[source] std::io::Error),
    #[error(transparent)]
    Supervision(#[from] SupervisionError),
    #[error("delivery record cannot be opened")]
    LedgerOpen(#[source] FileLedgerError),
    #[error("artifact store cannot be opened")]
    ArtifactOpen(#[source] amiss_controller::ArtifactError),
    #[error("check plan cannot be registered")]
    Plan(#[source] PlanError),
    #[error("HTTP evaluation configuration is invalid")]
    EvaluationConfiguration(#[source] EndpointConfigError),
    #[error("maintenance interval overflow")]
    MaintenanceInterval,
    #[error("delivery record maintenance panicked")]
    MaintenancePanicked(#[source] tokio::task::JoinError),
    #[error("delivery record maintenance failed")]
    Maintenance(#[source] FileLedgerError),
    #[error("evaluation unavailable")]
    EvaluationLedger(#[source] FileLedgerError),
    #[error("evaluation unavailable")]
    EvaluationRunner,
    #[error("evaluation unavailable")]
    EvaluationRegistry(#[source] RegistryError),
    #[error("evaluation unavailable")]
    EvaluationController(#[source] ControllerError<FileLedgerError>),
}

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
    artifacts: Arc<amiss_controller::FileArtifactStore>,
}

/// Runs one synchronous GitLab policy-job lane until shutdown.
///
/// # Errors
///
/// Returns an error when the lane cannot start or continue safely.
pub async fn run(config: ServiceConfig) -> Result<(), ServiceError> {
    let shutdown = shutdown_signal().map_err(ServiceError::ShutdownInstall)?;
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
        .map_err(ServiceError::Listener)?;
    let maintenance_stop = Arc::new(Notify::new());
    let component_stop = Arc::clone(&maintenance_stop);
    let component = maintain_ledger(ledger, Arc::clone(&maintenance_stop), operations.clone());
    let component = async move { component.await.map_err(SupervisionError::component) };
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
    .await?;
    Ok(())
}

fn prepare(config: ServiceConfig) -> Result<Prepared, ServiceError> {
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let ledger = Arc::new(
        FileLedgerRoot::open_with_clock(&config.ledger_root, config.ledger, Arc::clone(&clock))
            .map_err(ServiceError::LedgerOpen)?,
    );
    let artifacts = open_artifact_service(config.artifacts, Arc::clone(&clock))
        .map_err(ServiceError::ArtifactOpen)?;
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, config.scope, Arc::clone(&config.plan))
        .map_err(ServiceError::Plan)?;
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
        artifacts: Arc::clone(&artifacts.store),
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
    .map_err(ServiceError::EvaluationConfiguration)?;
    let router = artifact_routes(router, &artifacts);
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
        .ok_or(ServiceError::MaintenanceInterval)?;
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
        .map_err(ServiceError::MaintenancePanicked)?
        .map_err(ServiceError::Maintenance)?;
    operations.maintenance_runs.inc();
    operations.maintenance_removals.inc_by(
        removed
            .removed_records
            .saturating_add(removed.removed_reports)
            .saturating_add(removed.removed_temporary),
    );
    Ok(())
}

fn evaluate(lane: &Lane, request: EvaluationRequest<'_>) -> Response {
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
        Ok(_accepted) => result_response(handle(lane, untrusted)),
        Err(rejection) => rejection_status(rejection).into_response(),
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
        .map_err(ServiceError::EvaluationLedger)?;
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
    .ok_or(ServiceError::EvaluationRunner)?;
    let mut registry = AdapterRegistry::new();
    registry
        .register(Arc::clone(&lane.adapter))
        .map_err(ServiceError::EvaluationRegistry)?;
    let mut controller = Controller::new_with_clock(
        registry,
        lane.plans.clone(),
        ledger,
        runner,
        lane.ingress,
        clock,
    )
    .with_external_sink(Arc::new(lane.operations.clone()))
    .with_artifact_store(Arc::clone(&lane.artifacts));
    controller
        .handle(untrusted)
        .map_err(ServiceError::EvaluationController)
}

fn result_status<E>(result: Result<amiss_controller::HandleOutcome, E>) -> StatusCode {
    match result {
        Ok(outcome) if policy_job_accepted(&outcome) => StatusCode::NO_CONTENT,
        Ok(_) => StatusCode::PRECONDITION_FAILED,
        Err(_defect) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn result_response<E>(result: Result<amiss_controller::HandleOutcome, E>) -> Response {
    let artifact = result
        .as_ref()
        .ok()
        .and_then(|outcome| match outcome {
            amiss_controller::HandleOutcome::Published { artifact, .. }
            | amiss_controller::HandleOutcome::Duplicate { artifact, .. } => artifact.as_ref(),
            amiss_controller::HandleOutcome::InProgress { .. } => None,
        })
        .cloned();
    let status = result_status(result);
    let Some(artifact) = artifact else {
        return status.into_response();
    };
    artifact_headers(status, &artifact)
        .unwrap_or_else(|| StatusCode::SERVICE_UNAVAILABLE.into_response())
}

fn artifact_headers(status: StatusCode, artifact: &ArtifactReference) -> Option<Response> {
    let mut response = status.into_response();
    let component_root = artifact.locator.strip_suffix("/report")?;
    let mut links = vec![format!("<{}>; rel=\"amiss-report\"", artifact.locator)];
    if artifact.semantic_digest.is_some() {
        links.push(format!(
            "<{component_root}/semantic>; rel=\"amiss-semantic-input\""
        ));
    }
    if artifact.assessment_digest.is_some() {
        links.push(format!(
            "<{component_root}/assessment>; rel=\"amiss-assessment\""
        ));
    }
    response
        .headers_mut()
        .insert(header::LINK, HeaderValue::from_str(&links.join(", ")).ok()?);
    response
        .headers_mut()
        .insert("x-amiss-artifact-auth", HeaderValue::from_static("bearer"));
    response.headers_mut().insert(
        "x-amiss-artifact-expires-unix-millis",
        HeaderValue::from_str(&artifact.expires_at_unix_millis.to_string()).ok()?,
    );
    response.headers_mut().insert(
        "x-amiss-report-digest",
        HeaderValue::from_str(&artifact.report_digest.to_string()).ok()?,
    );
    if let Some(digest) = artifact.semantic_digest {
        response.headers_mut().insert(
            "x-amiss-semantic-input-digest",
            HeaderValue::from_str(&digest.to_string()).ok()?,
        );
    }
    if let Some(digest) = artifact.assessment_digest {
        response.headers_mut().insert(
            "x-amiss-assessment-digest",
            HeaderValue::from_str(&digest.to_string()).ok()?,
        );
    }
    if artifact.external_incomplete {
        response.headers_mut().insert(
            "x-amiss-external-assessment",
            HeaderValue::from_static("incomplete"),
        );
    } else if let Some(tally) = artifact.external_tally {
        response.headers_mut().insert(
            "x-amiss-external-assessment",
            HeaderValue::from_static("complete"),
        );
        for (name, count) in [
            ("x-amiss-external-refuted", tally.refuted),
            ("x-amiss-external-unproven", tally.unproven),
            ("x-amiss-external-reachable", tally.reachable),
        ] {
            response
                .headers_mut()
                .insert(name, HeaderValue::from_str(&count.to_string()).ok()?);
        }
    }
    Some(response)
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
