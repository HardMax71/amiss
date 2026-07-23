#![cfg(test)]

use amiss_controller::ProviderError;
use bytes::Bytes;
use http::{HeaderValue, Response, header::CONTENT_LENGTH};
use http_body_util::Full;
use serde_json::Value;

use super::{MAX_RESPONSE_BYTES, decode_json, map_status, validate_api_base};
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
    declared
        .headers_mut()
        .insert(CONTENT_LENGTH, HeaderValue::from_static("8388609"));
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
