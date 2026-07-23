use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use amiss_controller_service::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission, InboxError,
    IncomingDelivery, ReceiverConfig, ReceiverConfigError, router,
};
use axum::body::{Body, Bytes};
use axum::http::{Method, Request, StatusCode};
use tokio_stream::StreamExt as _;
use tower::ServiceExt as _;

use super::support::{
    BODY, DELIVERY_PATH, Fixture, TestAdmission, delivery_request, inbox_limits, receiver_config,
};

#[tokio::test]
async fn body_limit_returns_413_before_admission() {
    let mut config = receiver_config();
    config.max_body_bytes = BODY.len().saturating_sub(1);
    assert_rejected_before_admission(config, StatusCode::PAYLOAD_TOO_LARGE).await;
}

#[tokio::test]
async fn header_limit_returns_431_before_copy_or_admission() {
    let mut config = receiver_config();
    config.max_headers = 1;
    assert_rejected_before_admission(config, StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_delivery_capacity_rejects_before_reading_another_body()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::TempDir::new()?;
    let inbox = Arc::new(Mutex::new(amiss_controller_service::Inbox::open(
        directory.path(),
        inbox_limits(),
    )?));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let (started, observed) = tokio::sync::oneshot::channel();
    let admission: Arc<dyn DeliveryAdmission> = Arc::new(BlockingAdmission {
        calls: Arc::clone(&calls),
        release: Arc::clone(&release),
        started: Mutex::new(Some(started)),
    });
    let mut config = receiver_config();
    config.max_concurrent_deliveries = 1;
    let app = router(&config, inbox, admission)?;

    let first_app = app.clone();
    let first = tokio::spawn(async move {
        first_app
            .oneshot(delivery_request(
                Method::POST,
                DELIVERY_PATH,
                "delivery-1",
                BODY,
            ))
            .await
    });
    observed.await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let body_read = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&body_read);
    let stream =
        tokio_stream::iter([Ok::<_, Infallible>(Bytes::from_static(BODY))]).map(move |item| {
            observed.store(true, Ordering::SeqCst);
            item
        });
    let second = Request::builder()
        .method(Method::POST)
        .uri(DELIVERY_PATH)
        .header("x-provider-secret", super::support::SECRET)
        .header("x-source-id", "delivery-2")
        .body(Body::from_stream(stream))?;
    let response = app.oneshot(second).await?;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(!body_read.load(Ordering::SeqCst));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let (lock, ready) = &*release;
    *lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    ready.notify_one();
    assert_eq!(first.await??.status(), StatusCode::ACCEPTED);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn pending_body_times_out_and_releases_delivery_capacity()
-> Result<(), Box<dyn std::error::Error>> {
    let mut config = receiver_config();
    config.max_concurrent_deliveries = 1;
    let fixture = Fixture::new(&config, inbox_limits(), TestAdmission::accepting());
    let (body_read, observed) = tokio::sync::oneshot::channel();
    let mut body_read = Some(body_read);
    let stream = tokio_stream::once(Ok::<_, Infallible>(Bytes::new()))
        .chain(tokio_stream::pending())
        .map(move |item| {
            if let Some(body_read) = body_read.take() {
                let _ignored = body_read.send(());
            }
            item
        });
    let request = Request::builder()
        .method(Method::POST)
        .uri(DELIVERY_PATH)
        .header("x-provider-secret", super::support::SECRET)
        .header("x-source-id", "pending-delivery")
        .body(Body::from_stream(stream))?;
    let app = fixture.app.clone();
    let pending = tokio::spawn(async move { app.oneshot(request).await });

    observed.await?;
    let saturated = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            DELIVERY_PATH,
            "delivery-1",
            BODY,
        ))
        .await?;
    assert_eq!(saturated.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(fixture.admission.calls(), 0);

    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    assert_eq!(pending.await??.status(), StatusCode::REQUEST_TIMEOUT);
    let accepted = fixture
        .app
        .clone()
        .oneshot(delivery_request(
            Method::POST,
            DELIVERY_PATH,
            "delivery-1",
            BODY,
        ))
        .await?;
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    Ok(())
}

async fn assert_rejected_before_admission(config: ReceiverConfig, expected: StatusCode) {
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
    assert_eq!(response.status(), expected);
    assert_eq!(fixture.admission.calls(), 0);
    assert!(fixture.inbox.lock().unwrap().entries().unwrap().is_empty());
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
    let inbox = Arc::new(Mutex::new(
        amiss_controller_service::Inbox::open(directory.path(), inbox_limits()).unwrap(),
    ));
    let admission = Arc::new(TestAdmission::accepting());
    let receiver_admission: Arc<dyn DeliveryAdmission> = admission.clone();
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
        router(&config, Arc::clone(&inbox), Arc::clone(&receiver_admission),),
        Err(ReceiverConfigError::Limits)
    ));
    config.max_body_bytes = 1;
    config.max_concurrent_deliveries = 65;
    assert!(matches!(
        router(&config, Arc::clone(&inbox), Arc::clone(&receiver_admission),),
        Err(ReceiverConfigError::Limits)
    ));
    for (body, headers, header_bytes) in [
        (8 * 1_024 * 1_024 + 1, 8, 256),
        (16, 129, 256),
        (16, 8, 32 * 1_024 + 1),
    ] {
        let mut config = receiver_config();
        config.max_body_bytes = body;
        config.max_headers = headers;
        config.max_header_bytes = header_bytes;
        assert!(matches!(
            router(&config, Arc::clone(&inbox), Arc::clone(&receiver_admission),),
            Err(ReceiverConfigError::Limits)
        ));
    }
}

struct BlockingAdmission {
    calls: Arc<AtomicUsize>,
    release: Arc<(Mutex<bool>, Condvar)>,
    started: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl DeliveryAdmission for BlockingAdmission {
    fn admit(
        &self,
        _request: AdmissionRequest<'_>,
    ) -> Result<AdmittedDelivery, AdmissionRejection> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(started) = self
            .started
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ignored = started.send(());
        }
        let (lock, ready) = &*self.release;
        let mut released = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*released {
            released = ready
                .wait(released)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        Ok(AdmittedDelivery {
            route: "github-main".to_owned(),
            source_id: "delivery-1".to_owned(),
        })
    }
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
