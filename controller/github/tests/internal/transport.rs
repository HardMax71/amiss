#![cfg(test)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use amiss_controller::ProviderError;
use bytes::Bytes;
use http::{HeaderValue, Response, header::CONTENT_LENGTH};
use http_body_util::Full;
use serde_json::Value;

use super::{
    MAX_RESPONSE_BYTES, OperationDeadline, decode_json, execute_on_runtime, map_status,
    validate_api_base,
};
use crate::GitHubClientError;

#[test]
fn api_authority_is_derived_from_the_provider_instance() {
    assert_eq!(
        validate_api_base("https://api.github.com", "github.com"),
        Ok(())
    );
    assert_eq!(
        validate_api_base("https://github.example/api/v3", "github.example"),
        Ok(())
    );

    for (base, instance) in [
        ("https://attacker.invalid", "github.com"),
        ("https://github.com", "github.com"),
        ("https://api.github.com:443", "github.com"),
        ("https://github.example:8443/api/v3", "github.example"),
        ("https://api.github.example/api/v3", "github.example"),
        ("http://api.github.com", "github.com"),
        ("https://user@api.github.com", "github.com"),
        ("https://api.github.com?version=3", "github.com"),
    ] {
        assert_eq!(
            validate_api_base(base, instance),
            Err(GitHubClientError::Configuration)
        );
    }
}

#[test]
fn response_bodies_are_bounded_before_json_decoding() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut declared = Response::new(Full::new(Bytes::from_static(b"{}")));
    let declared_length = (MAX_RESPONSE_BYTES + 1).to_string();
    declared.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_bytes(declared_length.as_bytes()).unwrap(),
    );
    assert_eq!(
        runtime.block_on(decode_json::<Value, _>(declared)),
        Err(ProviderError::InvalidResponse)
    );

    let streamed = Response::new(Full::new(Bytes::from(vec![0; MAX_RESPONSE_BYTES + 1])));
    assert_eq!(
        runtime.block_on(decode_json::<Value, _>(streamed)),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn provider_statuses_have_stable_failure_classes() {
    assert_eq!(map_status(401), ProviderError::AuthorizationRevoked);
    assert_eq!(map_status(403), ProviderError::AuthorizationRevoked);
    assert_eq!(map_status(429), ProviderError::Unavailable);
    assert_eq!(map_status(503), ProviderError::Unavailable);
    assert_eq!(map_status(404), ProviderError::InvalidResponse);
}

#[test]
fn a_caller_timeout_cancels_a_request_that_has_not_started()
-> Result<(), Box<dyn std::error::Error>> {
    struct DropSignal(Option<mpsc::SyncSender<()>>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ignored = sender.send(());
            }
        }
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let (worker_started, started) = mpsc::sync_channel(0);
    let (release_worker, release) = mpsc::sync_channel(0);
    let blocker = runtime.spawn(async move {
        let _ignored = worker_started.send(());
        let _ignored = release.recv();
    });
    started.recv_timeout(Duration::from_secs(1))?;

    let polled = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&polled);
    let (dropped, request_dropped) = mpsc::sync_channel(1);
    let signal = DropSignal(Some(dropped));
    let request = async move {
        let _signal = signal;
        observed.store(true, Ordering::Release);
        Ok(())
    };
    let deadline = OperationDeadline::after(Duration::from_millis(25))?;

    assert_eq!(
        execute_on_runtime(&runtime, request, deadline),
        Err(ProviderError::Unavailable)
    );
    release_worker.send(())?;
    runtime.block_on(blocker)?;
    request_dropped.recv_timeout(Duration::from_secs(1))?;
    assert!(!polled.load(Ordering::Acquire));
    Ok(())
}
