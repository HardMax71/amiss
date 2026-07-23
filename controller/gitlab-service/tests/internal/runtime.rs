#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed runtime boundary fixtures must fail loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, CheckConclusion, ControllerClock, DeliveryRoute,
    FileLedgerConfig, HandleOutcome, IngressLimits, IngressPolicy, OpaqueId, PlanRegistry,
    ProviderAdapter, ProviderError, ProviderIdentity, ProviderInstance, ProviderNamespace,
    Publication, ReplayWindow, RunFailure, SignedTimePolicy, SystemClock, VerifiedDelivery,
};
use amiss_controller_git::GitFetchBounds;
use amiss_controller_service::{AdmissionRejection, DeliveryHeader, EvaluationRequest};
use axum::http::StatusCode;
use secrecy::SecretString;

use super::{Lane, evaluate, rejection_status, result_status};

#[test]
fn only_a_published_pass_is_an_http_success() {
    assert_eq!(
        result_status::<super::ServiceError>(Ok(HandleOutcome::Published(CheckConclusion::Pass))),
        StatusCode::NO_CONTENT
    );
    for conclusion in [
        CheckConclusion::Block,
        CheckConclusion::Superseded,
        CheckConclusion::Unavailable(RunFailure::Unavailable),
    ] {
        assert_eq!(
            result_status::<super::ServiceError>(Ok(HandleOutcome::Published(conclusion))),
            StatusCode::PRECONDITION_FAILED
        );
    }
    assert_eq!(
        result_status::<super::ServiceError>(Err(super::ServiceError("evaluation unavailable"))),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[test]
fn failed_authentication_never_opens_the_delivery_record() {
    let state = tempfile::TempDir::new().unwrap();
    let ledger_root = state.path().join("must-not-be-opened");
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
    let lane = Lane {
        route,
        adapter,
        plans: PlanRegistry::new(),
        ledger: FileLedgerConfig::new(Duration::from_secs(2), 32, replay).unwrap(),
        ledger_root: ledger_root.clone(),
        ingress,
        project_id: 101,
        git_username: "oauth2".to_owned(),
        git_token: SecretString::from("unused-git-token-fixture".to_owned()),
        git_bounds: GitFetchBounds::default(),
        bootstrap: state.path().join("unused-bootstrap"),
        scratch: state.path().to_path_buf(),
        bootstrap_timeout: Duration::from_secs(1),
        statement_validity: Duration::from_mins(5),
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
    assert!(!ledger_root.exists());
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
