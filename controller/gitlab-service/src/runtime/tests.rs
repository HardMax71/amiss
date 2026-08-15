#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed runtime boundary fixtures must fail loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, CheckConclusion, ControllerClock, DeliveryRoute,
    FileLedgerConfig, FileLedgerRoot, HandleOutcome, IngressLimits, IngressPolicy, OpaqueId,
    PlanRegistry, ProviderAdapter, ProviderError, ProviderIdentity, ProviderInstance,
    ProviderNamespace, Publication, ReplayWindow, RunFailure, SignedTimePolicy, SystemClock,
    VerifiedDelivery,
};
use amiss_controller_git::GitFetchBounds;
use amiss_controller_service::{AdmissionRejection, DeliveryHeader, EvaluationRequest, Operations};
use axum::http::StatusCode;
use secrecy::SecretString;
use tokio::sync::Notify;

use super::{
    LEDGER_MAINTENANCE_INTERVAL, Lane, ServiceError, cleanup_ledger, clone_secret, evaluate,
    maintain_ledger, maintenance_loop, rejection_status, result_status,
};
use secrecy::ExposeSecret as _;

#[test]
fn only_a_published_pass_is_an_http_success() {
    assert_eq!(
        result_status::<ServiceError>(Ok(HandleOutcome::Published(CheckConclusion::Pass))),
        StatusCode::NO_CONTENT
    );
    for conclusion in [
        CheckConclusion::Block,
        CheckConclusion::Superseded,
        CheckConclusion::Unavailable(RunFailure::Unavailable),
    ] {
        assert_eq!(
            result_status::<ServiceError>(Ok(HandleOutcome::Published(conclusion))),
            StatusCode::PRECONDITION_FAILED
        );
    }
    assert_eq!(
        result_status::<ServiceError>(Err(ServiceError("evaluation unavailable"))),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[test]
fn failed_authentication_never_touches_the_delivery_record() {
    let state = tempfile::TempDir::new().unwrap();
    let ledger_root = state.path().join("ledger");
    std::fs::create_dir(&ledger_root).unwrap();
    let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1)).unwrap();
    let ingress = IngressPolicy::new(
        IngressLimits::new(1_024, 8, 32 * 1_024).unwrap(),
        replay,
        Duration::from_secs(2),
    )
    .unwrap();
    let provider = ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("gitlab.example".to_owned()).unwrap(),
    };
    let route = DeliveryRoute {
        provider,
        trust_set: OpaqueId::new("gitlab-oidc".to_owned()).unwrap(),
        signed_time: SignedTimePolicy::Required(Duration::from_mins(5)),
    };
    let adapter: Arc<dyn ProviderAdapter> = Arc::new(RejectingAdapter {
        namespace: route.provider.namespace.clone(),
    });
    let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
    let ledger = FileLedgerRoot::open_with_clock(
        &ledger_root,
        FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
        Arc::clone(&clock),
    )
    .unwrap();
    let entries_before = entries(&ledger_root);
    let lane = Lane {
        route,
        adapter,
        plans: PlanRegistry::new(),
        ledger: Arc::new(ledger),
        clock,
        ingress,
        project_id: 101,
        git_username: "oauth2".to_owned(),
        git_token: SecretString::from("unused-git-token-fixture".to_owned()),
        git_bounds: GitFetchBounds::default(),
        bootstrap: state.path().join("unused-bootstrap"),
        scratch: state.path().to_path_buf(),
        bootstrap_timeout: Duration::from_secs(1),
        statement_validity: Duration::from_mins(5),
        operations: Operations::default(),
    };
    let headers = [DeliveryHeader {
        name: "authorization".to_owned(),
        value: b"Bearer invalid".to_vec(),
    }];

    assert_eq!(
        evaluate(
            &lane,
            EvaluationRequest {
                received_at_unix_millis: SystemClock.now_unix_millis().unwrap(),
                headers: &headers,
                body: br#"{"merge_request_iid":42}"#,
            },
        ),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(entries(&ledger_root), entries_before);
}

#[tokio::test(start_paused = true)]
async fn periodic_maintenance_cleans_without_stopping_the_lane() {
    let state = tempfile::TempDir::new().unwrap();
    let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1)).unwrap();
    let ledger = Arc::new(
        FileLedgerRoot::open(
            state.path(),
            FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
        )
        .unwrap(),
    );
    let leftover = state.path().join(".atomicwrite-session-leftover");
    std::fs::create_dir(&leftover).unwrap();
    let completed = Arc::new(Notify::new());
    let observed = Arc::clone(&completed);
    let stop = Arc::new(Notify::new());
    let operations = Operations::default();
    let observed_operations = operations.clone();
    let period = Duration::from_mins(1);
    let maintenance = tokio::spawn(maintenance_loop(period, Arc::clone(&stop), move || {
        let ledger = Arc::clone(&ledger);
        let completed = Arc::clone(&observed);
        let operations = operations.clone();
        async move {
            cleanup_ledger(ledger, operations).await?;
            completed.notify_one();
            Ok(())
        }
    }));

    assert!(leftover.is_dir());
    assert!(!maintenance.is_finished());
    tokio::time::advance(period).await;
    completed.notified().await;
    assert!(!leftover.exists());
    assert_eq!(observed_operations.maintenance_runs.get(), 1);
    assert_eq!(observed_operations.maintenance_removals.get(), 1);
    assert!(!maintenance.is_finished());

    stop.notify_one();
    assert_eq!(maintenance.await.unwrap(), Ok(()));
}

#[tokio::test(start_paused = true)]
async fn stop_waits_for_active_maintenance() {
    let stop = Arc::new(Notify::new());
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let observed_started = Arc::clone(&started);
    let observed_release = Arc::clone(&release);
    let period = Duration::from_mins(1);
    let maintenance = tokio::spawn(maintenance_loop(period, Arc::clone(&stop), move || {
        let started = Arc::clone(&observed_started);
        let release = Arc::clone(&observed_release);
        async move {
            started.notify_one();
            release.notified().await;
            Ok(())
        }
    }));

    tokio::time::advance(period).await;
    started.notified().await;
    stop.notify_one();
    tokio::task::yield_now().await;
    assert!(!maintenance.is_finished());

    release.notify_one();
    assert_eq!(maintenance.await.unwrap(), Ok(()));
}

#[tokio::test(start_paused = true)]
async fn periodic_maintenance_failure_stops_the_lane() {
    let state = tempfile::TempDir::new().unwrap();
    let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1)).unwrap();
    let ledger = Arc::new(
        FileLedgerRoot::open(
            state.path(),
            FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
        )
        .unwrap(),
    );
    std::fs::write(state.path().join("unknown"), b"invalid ledger entry").unwrap();
    let operations = Operations::default();
    let observed_operations = operations.clone();
    let period = Duration::from_mins(1);
    let maintenance = tokio::spawn(maintenance_loop(
        period,
        Arc::new(Notify::new()),
        move || cleanup_ledger(Arc::clone(&ledger), operations.clone()),
    ));

    assert!(!maintenance.is_finished());
    tokio::time::advance(period).await;
    assert_eq!(
        maintenance.await.unwrap(),
        Err(ServiceError("delivery record maintenance failed"))
    );
    assert_eq!(observed_operations.maintenance_runs.get(), 0);
    assert_eq!(observed_operations.maintenance_removals.get(), 0);
}

fn entries(root: &std::path::Path) -> Vec<std::ffi::OsString> {
    let mut entries = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    entries.sort_unstable();
    entries
}

#[test]
fn admission_rejections_have_stable_http_classes() {
    assert_eq!(
        rejection_status(AdmissionRejection::Malformed),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        rejection_status(AdmissionRejection::Unauthorized),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        rejection_status(AdmissionRejection::Forbidden),
        StatusCode::FORBIDDEN
    );
}

struct RejectingAdapter {
    namespace: ProviderNamespace,
}

impl ProviderAdapter for RejectingAdapter {
    fn namespace(&self) -> &ProviderNamespace {
        &self.namespace
    }

    fn authenticate(
        &self,
        _delivery: amiss_controller::IngressCheck<'_>,
    ) -> Result<VerifiedDelivery, ProviderError> {
        Err(ProviderError::Authentication)
    }

    fn refresh(&self, _delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        Err(ProviderError::Unavailable)
    }

    fn publish(
        &self,
        _delivery: &AuthenticatedDelivery,
        _publication: &Publication,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::Unavailable)
    }
}

/// The ledger maintenance entry runs its cleanup on the interval and stops
/// clean, observable through the operations counters it carries.
#[tokio::test(start_paused = true)]
async fn ledger_maintenance_binds_cleanup_to_the_interval() {
    let state = tempfile::TempDir::new().unwrap();
    let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1)).unwrap();
    let ledger = Arc::new(
        FileLedgerRoot::open(
            state.path(),
            FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
        )
        .unwrap(),
    );
    let leftover = state.path().join(".atomicwrite-session-leftover");
    std::fs::create_dir(&leftover).unwrap();
    let stop = Arc::new(Notify::new());
    let operations = Operations::default();
    let task = tokio::spawn(maintain_ledger(
        Arc::clone(&ledger),
        Arc::clone(&stop),
        operations.clone(),
    ));
    tokio::time::sleep(LEDGER_MAINTENANCE_INTERVAL + Duration::from_secs(1)).await;
    assert!(!leftover.exists(), "the interval ran one cleanup");
    assert_eq!(operations.maintenance_runs.get(), 1);
    stop.notify_one();
    assert_eq!(task.await.unwrap(), Ok(()));
}

/// The cloned credential exposes the same bytes, not a fresh empty secret.
#[test]
fn a_cloned_secret_keeps_its_bytes() {
    let secret = SecretString::from("glpat-dedicated".to_owned());
    assert_eq!(clone_secret(&secret).expose_secret(), "glpat-dedicated");
}

/// The service error displays its reason verbatim.
#[test]
fn a_service_error_displays_its_reason() {
    assert_eq!(
        ServiceError("GitLab API timeouts are invalid").to_string(),
        "GitLab API timeouts are invalid"
    );
}
