use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

use amiss_controller_service::{EvaluationConfig, Operations, evaluation_router};
use axum::body::{Body, Bytes};
use axum::http::{Method, Request, StatusCode};
use tokio_stream::StreamExt as _;
use tower::ServiceExt as _;

const PATH: &str = "/provider/evaluate";

#[tokio::test]
async fn exact_post_is_evaluated_after_normalization() -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let operations = Operations::default();
    let (app, _drain) = evaluation_router(
        &config(),
        Arc::new(AtomicBool::new(true)),
        operations.clone(),
        move |request| {
            observed.fetch_add(1, Ordering::SeqCst);
            if request.received_at_unix_millis > 0
                && request.body == b"request"
                && request
                    .headers
                    .iter()
                    .any(|header| header.name == "x-job" && header.value == b"42")
            {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::UNAUTHORIZED
            }
        },
    )?;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(PATH)
                .header("x-job", "42")
                .body(Body::from("request"))?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(operations.provider_requests.get(), 1);
    assert_eq!(operations.provider_acceptances.get(), 1);
    Ok(())
}

#[tokio::test]
async fn query_wrong_method_and_oversize_never_evaluate() -> Result<(), Box<dyn std::error::Error>>
{
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let operations = Operations::default();
    let (app, _drain) = evaluation_router(
        &config(),
        Arc::new(AtomicBool::new(true)),
        operations.clone(),
        move |_request| {
            observed.fetch_add(1, Ordering::SeqCst);
            StatusCode::NO_CONTENT
        },
    )?;

    for request in [
        Request::builder()
            .method(Method::GET)
            .uri(PATH)
            .body(Body::empty())?,
        Request::builder()
            .method(Method::POST)
            .uri(format!("{PATH}?job=42"))
            .body(Body::empty())?,
        Request::builder()
            .method(Method::POST)
            .uri(PATH)
            .body(Body::from(vec![0_u8; 17]))?,
    ] {
        let response = app.clone().oneshot(request).await?;
        assert_ne!(response.status(), StatusCode::NO_CONTENT);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(operations.provider_requests.get(), 2);
    assert_eq!(operations.provider_refusals.get(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_evaluation_capacity_rejects_without_starting_more_work()
-> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let started = Arc::new(tokio::sync::Notify::new());
    let observed_calls = Arc::clone(&calls);
    let observed_release = Arc::clone(&release);
    let observed_started = Arc::clone(&started);
    let ready = Arc::new(AtomicBool::new(true));
    let mut limited = config();
    limited.max_concurrent_evaluations = 1;
    let (app, _drain) = evaluation_router(
        &limited,
        Arc::clone(&ready),
        Operations::default(),
        move |_request| {
            observed_calls.fetch_add(1, Ordering::SeqCst);
            observed_started.notify_one();
            let (lock, ready) = &*observed_release;
            let mut released = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while !*released {
                released = ready
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            StatusCode::NO_CONTENT
        },
    )?;

    let first_app = app.clone();
    let first_request = Request::builder()
        .method(Method::POST)
        .uri(PATH)
        .body(Body::from("first"))?;
    let first = tokio::spawn(async move { first_app.oneshot(first_request).await });
    tokio::time::timeout(std::time::Duration::from_secs(2), started.notified()).await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(PATH)
                .body(Body::from("second"))?,
        )
        .await?;
    assert_eq!(second.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    ready.store(false, Ordering::Release);
    let (lock, ready) = &*release;
    *lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    ready.notify_one();
    assert_eq!(first.await??.status(), StatusCode::NO_CONTENT);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drain_waits_for_detached_evaluation() -> Result<(), Box<dyn std::error::Error>> {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let started = Arc::new(tokio::sync::Notify::new());
    let observed_release = Arc::clone(&release);
    let observed_started = Arc::clone(&started);
    let mut limited = config();
    limited.max_concurrent_evaluations = 1;
    let (app, drain) = evaluation_router(
        &limited,
        Arc::new(AtomicBool::new(true)),
        Operations::default(),
        move |_request| {
            observed_started.notify_one();
            let (lock, ready) = &*observed_release;
            let mut released = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while !*released {
                released = ready
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            StatusCode::NO_CONTENT
        },
    )?;
    let request = tokio::spawn(
        app.oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(PATH)
                .body(Body::from("request"))?,
        ),
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), started.notified()).await?;
    request.abort();
    assert!(request.await.is_err());

    let draining = tokio::spawn(drain.wait());
    tokio::task::yield_now().await;
    assert!(!draining.is_finished());
    let (lock, ready) = &*release;
    *lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    ready.notify_one();

    let _guard = tokio::time::timeout(std::time::Duration::from_secs(2), draining).await???;
    Ok(())
}

#[tokio::test]
async fn probes_and_evaluation_follow_readiness() -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let ready = Arc::new(AtomicBool::new(false));
    let operations = Operations::default();
    let (app, _drain) = evaluation_router(
        &config(),
        Arc::clone(&ready),
        operations.clone(),
        move |_request| {
            observed.fetch_add(1, Ordering::SeqCst);
            StatusCode::NO_CONTENT
        },
    )?;

    for (path, expected) in [
        ("/healthz", StatusCode::OK),
        ("/readyz", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(path)
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), expected);
    }
    let unavailable = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(PATH)
                .header(axum::http::header::CONTENT_LENGTH, "17")
                .body(Body::from(vec![0_u8; 17]))?,
        )
        .await?;
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(operations.provider_requests.get(), 1);
    assert_eq!(operations.provider_unavailable.get(), 1);

    ready.store(true, Ordering::Release);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/readyz")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(operations.provider_requests.get(), 1);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn pending_body_times_out_and_releases_evaluation_capacity()
-> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = Arc::clone(&calls);
    let mut limited = config();
    limited.max_concurrent_evaluations = 1;
    let (app, _drain) = evaluation_router(
        &limited,
        Arc::new(AtomicBool::new(true)),
        Operations::default(),
        move |_request| {
            observed_calls.fetch_add(1, Ordering::SeqCst);
            StatusCode::NO_CONTENT
        },
    )?;
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
        .uri(PATH)
        .body(Body::from_stream(stream))?;
    let pending_app = app.clone();
    let pending = tokio::spawn(async move { pending_app.oneshot(request).await });

    observed.await?;
    let saturated = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(PATH)
                .body(Body::from("second"))?,
        )
        .await?;
    assert_eq!(saturated.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    assert_eq!(pending.await??.status(), StatusCode::REQUEST_TIMEOUT);
    let evaluated = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(PATH)
                .body(Body::from("after-timeout"))?,
        )
        .await?;
    assert_eq!(evaluated.status(), StatusCode::NO_CONTENT);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn invalid_endpoint_configuration_is_rejected() {
    for path in ["", "/", "/healthz", "/metrics", "/readyz", "//provider"] {
        let mut invalid = config();
        invalid.path = path.to_owned();
        assert!(
            evaluation_router(
                &invalid,
                Arc::new(AtomicBool::new(true)),
                Operations::default(),
                |_request| StatusCode::OK,
            )
            .is_err()
        );
    }
    let mut invalid = config();
    invalid.max_body_bytes = 0;
    assert!(
        evaluation_router(
            &invalid,
            Arc::new(AtomicBool::new(true)),
            Operations::default(),
            |_request| StatusCode::OK,
        )
        .is_err()
    );
    let mut invalid = config();
    invalid.max_concurrent_evaluations = 0;
    assert!(
        evaluation_router(
            &invalid,
            Arc::new(AtomicBool::new(true)),
            Operations::default(),
            |_request| StatusCode::OK,
        )
        .is_err()
    );
    invalid.max_concurrent_evaluations = 65;
    assert!(
        evaluation_router(
            &invalid,
            Arc::new(AtomicBool::new(true)),
            Operations::default(),
            |_request| StatusCode::OK,
        )
        .is_err()
    );
    for (body, headers, header_bytes) in [
        (8 * 1_024 * 1_024 + 1, 8, 256),
        (16, 129, 256),
        (16, 8, 32 * 1_024 + 1),
    ] {
        let mut invalid = config();
        invalid.max_body_bytes = body;
        invalid.max_headers = headers;
        invalid.max_header_bytes = header_bytes;
        assert!(
            evaluation_router(
                &invalid,
                Arc::new(AtomicBool::new(true)),
                Operations::default(),
                |_request| StatusCode::OK,
            )
            .is_err()
        );
    }
}

fn config() -> EvaluationConfig {
    EvaluationConfig {
        path: PATH.to_owned(),
        max_body_bytes: 16,
        max_headers: 8,
        max_header_bytes: 256,
        max_concurrent_evaluations: 2,
    }
}
