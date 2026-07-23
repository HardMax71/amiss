#![cfg(test)]

use std::io::Cursor;
use std::time::Duration;

use amiss_controller::ProviderError;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};
use secrecy::{ExposeSecret as _, SecretSlice};
use serde::Deserialize;

use super::{
    AppCredential, MAX_RESPONSE_BYTES, OperationDeadline, Transport, app_jwt, bounded_bytes,
    map_status, mint_status, validate_api_base,
};
use crate::{GitHubClientError, GitHubTimeouts};

#[test]
fn api_authority_is_derived_from_the_provider_instance() {
    assert_eq!(
        validate_api_base("https://api.github.com", "github.com"),
        Ok("https://api.github.com".to_owned())
    );
    assert_eq!(
        validate_api_base("https://github.example/api/v3", "github.example"),
        Ok("https://github.example/api/v3".to_owned())
    );
    assert_eq!(
        validate_api_base("https://github.example/api/v3/", "github.example"),
        Ok("https://github.example/api/v3".to_owned())
    );

    for (base, instance, reason) in [
        (
            "https://attacker.invalid",
            "github.com",
            "the API base names the wrong host",
        ),
        (
            "https://github.com",
            "github.com",
            "the API base names the wrong host",
        ),
        (
            "https://api.github.com:443",
            "github.com",
            "the API base must not name a port",
        ),
        (
            "https://github.example:8443/api/v3",
            "github.example",
            "the API base must not name a port",
        ),
        (
            "https://api.github.example/api/v3",
            "github.example",
            "the API base names the wrong host",
        ),
        (
            "http://api.github.com",
            "github.com",
            "the API base must use https",
        ),
        (
            "https://user@api.github.com",
            "github.com",
            "the API base must not carry credentials",
        ),
        (
            "https://api.github.com?version=3",
            "github.com",
            "the API base must not carry a query or fragment",
        ),
    ] {
        assert_eq!(
            validate_api_base(base, instance),
            Err(GitHubClientError::Configuration(reason))
        );
    }
}

#[test]
fn response_bodies_are_bounded_before_json_decoding() {
    assert_eq!(
        bounded_bytes(Some(MAX_RESPONSE_BYTES + 1), Cursor::new(b"{}")),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        bounded_bytes(None, Cursor::new(vec![0_u8; MAX_RESPONSE_BYTES + 1])),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        bounded_bytes(Some(2), Cursor::new(b"{}".to_vec())),
        Ok(b"{}".to_vec())
    );
}

#[test]
fn provider_statuses_have_stable_failure_classes() {
    assert_eq!(map_status(401), ProviderError::AuthorizationRevoked);
    assert_eq!(map_status(403), ProviderError::AuthorizationRevoked);
    assert_eq!(map_status(429), ProviderError::Unavailable);
    assert_eq!(map_status(503), ProviderError::Unavailable);
    assert_eq!(map_status(404), ProviderError::InvalidResponse);
    assert_eq!(mint_status(401), ProviderError::Authentication);
    assert_eq!(mint_status(403), ProviderError::Authentication);
    assert_eq!(mint_status(503), ProviderError::Unavailable);
    assert_eq!(mint_status(404), ProviderError::InvalidResponse);
}

#[test]
fn app_jwt_binds_the_app_and_a_bounded_lifetime() {
    let credential = AppCredential {
        key: EncodingKey::from_rsa_pem(include_bytes!("../fixtures/private.pem")).unwrap(),
        app_id: 99,
        installation_id: 7,
    };
    let token = app_jwt(&credential).unwrap();
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["99"]);
    let decoded = jsonwebtoken::decode::<Claims>(
        token.expose_secret(),
        &DecodingKey::from_rsa_pem(include_bytes!("../fixtures/public.pem")).unwrap(),
        &validation,
    )
    .unwrap();
    assert_eq!(decoded.claims.iss, "99");
    assert_eq!(decoded.claims.exp - decoded.claims.iat, 600);
}

#[derive(Deserialize)]
struct Claims {
    iat: u64,
    exp: u64,
    iss: String,
}

#[test]
fn an_expired_deadline_fails_before_any_transport_io() {
    let timeouts = GitHubTimeouts::new(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let transport = Transport::new(
        99,
        7,
        SecretSlice::from(include_bytes!("../fixtures/private.pem").to_vec()),
        "github.com",
        "https://api.github.com",
        timeouts,
    )
    .unwrap();
    let deadline = OperationDeadline::after(Duration::ZERO).unwrap();
    assert_eq!(
        transport
            .get::<serde_json::Value>("/rate_limit", deadline)
            .err(),
        Some(ProviderError::Unavailable)
    );
}
