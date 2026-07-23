use amiss_controller_service::{AdmissionRejection, ClaimOutcome};
use axum::body::Body;
use axum::http::{Method, StatusCode};
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
}

#[tokio::test]
async fn health_only_reports_process_liveness() {
    let fixture = Fixture::new(
        &receiver_config(),
        inbox_limits(),
        TestAdmission::rejecting(AdmissionRejection::Forbidden),
    );
    let response = fixture
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fixture.admission.calls(), 0);
}
