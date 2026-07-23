use amiss_controller_service::{InboxError, IncomingDelivery, ReceiverConfigError, router};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt as _;

use super::support::{
    BODY, DELIVERY_PATH, Fixture, TestAdmission, delivery_request, inbox_limits, receiver_config,
};

#[tokio::test]
async fn body_limit_returns_413_before_admission() {
    let mut config = receiver_config();
    config.max_body_bytes = BODY.len().saturating_sub(1);
    let fixture = Fixture::new(&config, inbox_limits(), TestAdmission::accepting());
    let response = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            DELIVERY_PATH,
            "delivery-1",
            BODY,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(fixture.admission.calls(), 0);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[tokio::test]
async fn header_limit_returns_431_before_copy_or_admission() {
    let mut config = receiver_config();
    config.max_headers = 1;
    let fixture = Fixture::new(&config, inbox_limits(), TestAdmission::accepting());
    let response = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            DELIVERY_PATH,
            "delivery-1",
            BODY,
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
    );
    assert_eq!(fixture.admission.calls(), 0);
}

#[tokio::test]
async fn full_inbox_returns_503_after_successful_admission() {
    let mut limits = inbox_limits();
    limits.max_records = 1;
    let fixture = Fixture::new(&receiver_config(), limits, TestAdmission::accepting());
    fixture
        .inbox
        .lock()
        .unwrap()
        .enqueue(IncomingDelivery {
            route: "github-main",
            source_id: "already-present",
            received_at_unix_millis: 1,
            headers: &[],
            body: b"existing",
        })
        .unwrap();

    let response = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            DELIVERY_PATH,
            "delivery-2",
            BODY,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(fixture.admission.calls(), 1);
}

#[test]
fn invalid_paths_and_limits_are_data_errors_not_panics() {
    let directory = tempfile::TempDir::new().unwrap();
    let inbox = std::sync::Arc::new(std::sync::Mutex::new(
        amiss_controller_service::Inbox::open(directory.path(), inbox_limits()).unwrap(),
    ));
    let admission = std::sync::Arc::new(TestAdmission::accepting());
    let receiver_admission: std::sync::Arc<dyn amiss_controller_service::DeliveryAdmission> =
        admission.clone();
    for path in [
        "",
        "/",
        "/healthz",
        "/provider/{route}",
        "/provider//delivery",
    ] {
        let mut config = receiver_config();
        config.delivery_path = path.to_owned();
        assert!(matches!(
            router(&config, inbox.clone(), receiver_admission.clone(),),
            Err(ReceiverConfigError::Path)
        ));
    }
    let mut config = receiver_config();
    config.max_body_bytes = 0;
    assert!(matches!(
        router(&config, inbox, receiver_admission,),
        Err(ReceiverConfigError::Limits)
    ));
}

#[test]
fn public_error_text_contains_no_request_material() {
    for text in [
        ReceiverConfigError::Path.to_string(),
        ReceiverConfigError::Limits.to_string(),
        InboxError::Conflict.to_string(),
    ] {
        assert!(!text.contains(super::support::SECRET));
        assert!(!text.contains(std::str::from_utf8(BODY).unwrap()));
    }
}

#[tokio::test]
async fn byte_header_ceiling_is_independent_of_header_count() {
    let mut config = receiver_config();
    config.max_header_bytes = 8;
    let fixture = Fixture::new(&config, inbox_limits(), TestAdmission::accepting());
    let request = Request::builder()
        .method(Method::POST)
        .uri(DELIVERY_PATH)
        .header("x", "12345678")
        .body(Body::from(BODY))
        .unwrap();
    let response = fixture.app.clone().oneshot(request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
    );
    assert_eq!(fixture.admission.calls(), 0);
}
