use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{ArtifactBundle, ControllerClock, ControllerEvaluationId};
use amiss_controller_fixtures::clock::TestClock;
use amiss_controller_service::{
    ArtifactFiles, ArtifactLimits, EndpointConfig, artifact_routes, load_artifact_service,
    open_artifact_service,
};
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use serde_json::json;
use tower::ServiceExt as _;

const TOKEN: &str = "artifact-bearer-token-with-fixed-test-entropy";
const ARTIFACT_LIMITS: ArtifactLimits = ArtifactLimits {
    retention: Duration::from_secs(1),
    records: 4,
    bytes: 1_048_576,
    record_bytes: 524_288,
};

#[test]
fn configuration_rejects_ambiguous_urls_and_weak_tokens() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    let token_file = root.path().join("artifact.token");
    std::fs::write(&token_file, TOKEN)?;
    let endpoint = EndpointConfig {
        path: "/provider/evaluate".to_owned(),
        max_body_bytes: 1_024,
        max_headers: 16,
        max_header_bytes: 4_096,
        max_concurrent_requests: 2,
    };
    for base_url in [
        "http://amiss.example/artifacts",
        "https://amiss.example/",
        "https://amiss.example/artifacts/",
        "https://amiss.example/artifacts?download=1",
    ] {
        let files: ArtifactFiles = serde_json::from_value(json!({
            "base_url": base_url,
            "bearer_token_file": token_file
        }))?;
        assert!(
            load_artifact_service(
                &files,
                root.path().join("artifacts"),
                ARTIFACT_LIMITS,
                &endpoint,
            )
            .is_err(),
            "{base_url}"
        );
    }

    std::fs::write(&token_file, b"too-short")?;
    let files: ArtifactFiles = serde_json::from_value(json!({
        "base_url": "https://amiss.example/artifacts",
        "bearer_token_file": token_file
    }))?;
    assert!(
        load_artifact_service(
            &files,
            root.path().join("artifacts"),
            ARTIFACT_LIMITS,
            &endpoint,
        )
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn retained_bytes_require_the_exact_bearer_and_expire_at_the_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let artifact_root = root.path().join("artifacts");
    std::fs::create_dir(&artifact_root)?;
    let token_file = root.path().join("artifact.token");
    std::fs::write(&token_file, TOKEN)?;
    let files: ArtifactFiles = serde_json::from_value(json!({
        "base_url": "https://amiss.example/private/artifacts",
        "bearer_token_file": token_file
    }))?;
    let endpoint = EndpointConfig {
        path: "/provider/evaluate".to_owned(),
        max_body_bytes: 1_024,
        max_headers: 16,
        max_header_bytes: 4_096,
        max_concurrent_requests: 2,
    };
    let config = load_artifact_service(&files, artifact_root, ARTIFACT_LIMITS, &endpoint)?;
    let clock = TestClock::at(1_000);
    let controller_clock: Arc<dyn ControllerClock> = clock.clone();
    let service = open_artifact_service(config, controller_clock)?;
    let report = serde_json::to_vec(&json!({
        "feedback": (0..11).collect::<Vec<_>>()
    }))?;
    let retained = service.store.retain(
        &ControllerEvaluationId::new("evaluation/http".to_owned()).unwrap(),
        ArtifactBundle {
            report: &report,
            plan: None,
            evidence: None,
            assessment: None,
            external_tally: None,
            external_incomplete: false,
        },
    )?;
    let app = artifact_routes(Router::new(), &service);
    let path = format!("/private/artifacts/{}/report", retained.id);

    let unauthenticated = app
        .clone()
        .oneshot(Request::builder().uri(&path).body(Body::empty())?)
        .await?;
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        unauthenticated.headers().get(header::WWW_AUTHENTICATE),
        Some(&header::HeaderValue::from_static("Bearer"))
    );

    let rejected = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header(header::AUTHORIZATION, "Bearer incorrect-token")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&path)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    assert_eq!(
        to_bytes(response.into_body(), 1_048_576).await?,
        report.as_slice()
    );

    let queried = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{path}?download=1"))
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(queried.status(), StatusCode::BAD_REQUEST);

    clock.set(retained.expires_at_unix_millis);
    let expired = app
        .oneshot(
            Request::builder()
                .uri(&path)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(expired.status(), StatusCode::NOT_FOUND);
    Ok(())
}
