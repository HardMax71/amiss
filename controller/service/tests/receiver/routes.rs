use std::sync::atomic::Ordering;

use amiss_controller_service::{AdmissionRejection, ClaimOutcome};
use axum::body::{self, Body};
use axum::http::{Method, StatusCode, header};
use tower::ServiceExt as _;

use super::support::{
    BODY, DELIVERY_PATH, Fixture, SECRET, TestAdmission, delivery_request, inbox_limits,
    receiver_config,
};

#[tokio::test]
async fn accepted_response_follows_durable_storage() {
    let fixture = Fixture::new(
        &receiver_config(),
        inbox_limits(),
        TestAdmission::accepting(),
    );
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
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(fixture.admission.calls(), 1);
    assert_eq!(fixture.operations.provider_requests.get(), 1);
    assert_eq!(fixture.operations.provider_acceptances.get(), 1);

    let claimed = {
        let mut inbox = fixture.inbox.lock().unwrap();
        let ClaimOutcome::Claimed(claimed) = inbox.claim(0).unwrap() else {
            panic!("accepted delivery was not durable");
        };
        claimed
    };
    assert_eq!(claimed.delivery.route, "github-main");
    assert_eq!(claimed.delivery.source_id, "delivery-1");
    assert!(claimed.delivery.received_at_unix_millis > 0);
    assert_eq!(claimed.delivery.body, BODY);
    assert!(
        claimed.delivery.headers.iter().any(|header| {
            header.name == "x-provider-secret" && header.value == SECRET.as_bytes()
        })
    );
}

#[tokio::test]
async fn duplicate_bytes_are_accepted_without_a_second_row() {
    let fixture = Fixture::new(
        &receiver_config(),
        inbox_limits(),
        TestAdmission::accepting(),
    );
    for _ in 0..2 {
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
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
    assert_eq!(fixture.admission.calls(), 2);
    assert_eq!(fixture.inbox.lock().unwrap().entries().unwrap().len(), 1);
}

#[tokio::test]
async fn admission_rejection_never_reaches_storage() {
    let fixture = Fixture::new(
        &receiver_config(),
        inbox_limits(),
        TestAdmission::rejecting(AdmissionRejection::Unauthorized),
    );
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
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(fixture.operations.provider_requests.get(), 1);
    assert_eq!(fixture.operations.provider_refusals.get(), 1);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
}

#[tokio::test]
async fn only_the_configured_post_path_reaches_admission() {
    let fixture = Fixture::new(
        &receiver_config(),
        inbox_limits(),
        TestAdmission::accepting(),
    );
    let wrong_method = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::GET,
            DELIVERY_PATH,
            "delivery-1",
            BODY,
        ))
        .await
        .unwrap();
    let wrong_path = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            "/provider/other",
            "delivery-1",
            BODY,
        ))
        .await
        .unwrap();
    let query_route = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            "/provider/delivery?route=other",
            "delivery-1",
            BODY,
        ))
        .await
        .unwrap();
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(wrong_path.status(), StatusCode::NOT_FOUND);
    assert_eq!(query_route.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fixture.admission.calls(), 0);
    assert_eq!(fixture.operations.provider_requests.get(), 1);
    assert_eq!(fixture.operations.provider_refusals.get(), 1);
}

#[tokio::test]
async fn probes_separate_liveness_from_local_readiness() {
    let mut config = receiver_config();
    config.max_body_bytes = BODY.len().saturating_sub(1);
    let fixture = Fixture::new(
        &config,
        inbox_limits(),
        TestAdmission::rejecting(AdmissionRejection::Forbidden),
    );
    fixture.ready.store(false, Ordering::Release);
    for (path, expected) in [
        ("/healthz", StatusCode::OK),
        ("/readyz", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let response = fixture
            .app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method(Method::GET)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let mut draining_request = delivery_request(Method::POST, DELIVERY_PATH, "delivery-1", BODY);
    draining_request.headers_mut().insert(
        header::CONTENT_LENGTH,
        axum::http::HeaderValue::from_str(&BODY.len().to_string()).unwrap(),
    );
    let draining_delivery = fixture.app.clone().oneshot(draining_request).await.unwrap();
    assert_eq!(draining_delivery.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(fixture.operations.provider_requests.get(), 1);
    assert_eq!(fixture.operations.provider_unavailable.get(), 1);
    assert_eq!(fixture.admission.calls(), 0);

    fixture.ready.store(true, Ordering::Release);
    let response = fixture
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fixture.admission.calls(), 0);

    let metrics = fixture
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(metrics.status(), StatusCode::OK);
    assert_eq!(
        metrics.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/openmetrics-text; version=1.0.0; charset=utf-8"
    );
    let body = body::to_bytes(metrics.into_body(), 16 * 1_024)
        .await
        .unwrap();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("amiss_controller_provider_requests_total 1"));
    assert_eq!(fixture.operations.provider_requests.get(), 1);
}
