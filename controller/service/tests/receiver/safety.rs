use amiss_controller_service::AdmissionRejection;
use axum::body;
use axum::http::{Method, StatusCode};
use tower::ServiceExt as _;

use super::support::{
    BODY, DELIVERY_PATH, Fixture, SECRET, TestAdmission, delivery_request, inbox_limits,
    receiver_config,
};

#[tokio::test]
async fn response_errors_never_echo_authentication_or_body_bytes() {
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
    let response_body = body::to_bytes(response.into_body(), 1_024).await.unwrap();
    assert!(
        !response_body
            .windows(SECRET.len())
            .any(|part| part == SECRET.as_bytes())
    );
    assert!(!response_body.windows(BODY.len()).any(|part| part == BODY));
    let debug = format!("{:?}", AdmissionRejection::Unauthorized);
    assert!(!debug.contains(SECRET));
    assert!(!debug.contains(std::str::from_utf8(BODY).unwrap()));
}

#[tokio::test]
async fn source_conflicts_are_reported_without_overwriting_the_row() {
    let fixture = Fixture::new(
        &receiver_config(),
        inbox_limits(),
        TestAdmission::accepting(),
    );
    let first = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            DELIVERY_PATH,
            "delivery-1",
            b"first",
        ))
        .await
        .unwrap();
    let conflict = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            DELIVERY_PATH,
            "delivery-1",
            b"second",
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
}
