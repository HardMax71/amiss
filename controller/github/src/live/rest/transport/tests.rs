#![cfg(test)]

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use amiss_controller::{ForgeNegative, ProviderError};
use amiss_controller_fixtures::{RsaKeys, rsa_keys};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Validation};
use reqwest::blocking::Client;
use secrecy::{ExposeSecret as _, SecretSlice, SecretString};
use serde::Deserialize;

use super::{
    AppCredential, MAX_API_BASE_BYTES, MAX_RESPONSE_BYTES, MintedToken, OperationDeadline,
    Transport, app_jwt, classified, map_error, map_status, rate_limited, settled,
    validate_api_base,
};
use crate::{GitHubClientError, GitHubTimeouts};

static RSA_KEYS: LazyLock<RsaKeys> =
    LazyLock::new(|| rsa_keys().expect("the RSA fixture is valid"));

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
fn provider_statuses_have_stable_failure_classes() {
    assert_eq!(map_status(401), ProviderError::AuthorizationRevoked);
    assert_eq!(map_status(403), ProviderError::AuthorizationRevoked);
    assert_eq!(map_status(429), ProviderError::Unavailable);
    assert_eq!(map_status(503), ProviderError::Unavailable);
    assert_eq!(map_status(404), ProviderError::InvalidResponse);
}

/// The last-in-quota auth 403 reads rate-limited by design: the retry heals
/// that within a window, while a false revocation would publish and stand.
#[test]
fn a_rate_limited_403_is_unavailable_not_revoked() {
    let mut spent = reqwest::header::HeaderMap::new();
    spent.insert("x-ratelimit-remaining", "0".parse().unwrap());
    assert_eq!(
        settled(403, &spent, ProviderError::AuthorizationRevoked),
        Err(ProviderError::Unavailable)
    );
    assert_eq!(
        settled(403, &spent, ProviderError::Authentication),
        Err(ProviderError::Unavailable)
    );
    assert_eq!(
        settled(401, &spent, ProviderError::Authentication),
        Err(ProviderError::Authentication)
    );
    assert_eq!(
        settled(429, &spent, ProviderError::AuthorizationRevoked),
        Err(ProviderError::Unavailable)
    );
    assert_eq!(
        settled(200, &spent, ProviderError::AuthorizationRevoked),
        Ok(())
    );
}

#[test]
fn the_rate_limit_signature_is_retry_after_or_a_spent_quota() {
    let mut spent = reqwest::header::HeaderMap::new();
    spent.insert("x-ratelimit-remaining", "0".parse().unwrap());
    let mut live = reqwest::header::HeaderMap::new();
    live.insert("x-ratelimit-remaining", "4999".parse().unwrap());
    let mut asked = reqwest::header::HeaderMap::new();
    asked.insert(reqwest::header::RETRY_AFTER, "60".parse().unwrap());
    assert!(rate_limited(&spent));
    assert!(rate_limited(&asked));
    assert!(!rate_limited(&live));
    assert!(!rate_limited(&reqwest::header::HeaderMap::new()));
}

#[test]
fn app_jwt_binds_the_app_and_a_bounded_lifetime() {
    let credential = AppCredential {
        key: EncodingKey::from_rsa_pem(&RSA_KEYS.private_pem).unwrap(),
        app_id: 99,
        installation_id: 7,
    };
    let token = app_jwt(&credential).unwrap();
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["99"]);
    let decoded = jsonwebtoken::decode::<Claims>(
        token.expose_secret(),
        &DecodingKey::from_rsa_pem(&RSA_KEYS.public_pem).unwrap(),
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
    let timeouts = GitHubTimeouts::new(Duration::from_secs(1), Duration::from_secs(1)).unwrap();
    let transport = Transport::new(
        99,
        7,
        SecretSlice::from(RSA_KEYS.private_pem.clone()),
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

fn offline_transport(minted: Option<MintedToken>) -> Transport {
    Transport {
        client: Client::new(),
        api_base: "https://api.github.com".to_owned(),
        app: AppCredential {
            key: EncodingKey::from_rsa_pem(&RSA_KEYS.private_pem).unwrap(),
            app_id: 99,
            installation_id: 7,
        },
        minted: Mutex::new(minted),
        operation_timeout: Duration::ZERO,
    }
}

#[test]
fn the_ceilings_are_the_documented_values() {
    assert_eq!(MAX_RESPONSE_BYTES, 8_388_608);
    assert_eq!(MAX_API_BASE_BYTES, 2_048);
}

#[test]
fn only_the_success_range_settles() {
    let none = reqwest::header::HeaderMap::new();
    assert_eq!(
        settled(200, &none, ProviderError::AuthorizationRevoked),
        Ok(())
    );
    assert_eq!(settled(299, &none, ProviderError::Authentication), Ok(()));
    assert_eq!(
        settled(199, &none, ProviderError::AuthorizationRevoked),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        settled(300, &none, ProviderError::AuthorizationRevoked),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        settled(403, &none, ProviderError::AuthorizationRevoked),
        Err(ProviderError::AuthorizationRevoked)
    );
    assert_eq!(
        settled(401, &none, ProviderError::Authentication),
        Err(ProviderError::Authentication)
    );
    assert_eq!(
        settled(503, &none, ProviderError::Authentication),
        Err(ProviderError::Unavailable)
    );
}

#[test]
fn a_route_is_one_absolute_unrepeated_path() {
    let transport = offline_transport(None);
    assert!(
        transport
            .url("/rate_limit")
            .is_ok_and(|url| url.as_str().ends_with("/rate_limit"))
    );
    assert_eq!(
        transport.url("rate_limit").err(),
        Some(ProviderError::InvalidResponse)
    );
    assert_eq!(
        transport.url("//attacker.invalid").err(),
        Some(ProviderError::InvalidResponse)
    );
}

#[test]
fn a_fresh_token_is_reused_and_an_empty_cache_must_mint() {
    let cached = offline_transport(Some(MintedToken {
        token: SecretString::from("cached"),
        minted_at: Instant::now(),
    }));
    let deadline = OperationDeadline::after(Duration::ZERO).unwrap();
    assert_eq!(
        cached
            .token(deadline)
            .map(|token| token.expose_secret().to_owned()),
        Ok("cached".to_owned())
    );

    let empty = offline_transport(None);
    assert_eq!(
        empty.installation_access_token().err(),
        Some(ProviderError::Unavailable),
        "an empty cache mints, and the elapsed deadline refuses before any io"
    );
}

#[test]
fn a_request_that_cannot_even_build_is_an_invalid_response() {
    let defect = Client::new().get("no-scheme").build().unwrap_err();
    assert!(defect.is_builder());
    assert_eq!(map_error(&defect), ProviderError::InvalidResponse);
}

#[test]
fn the_api_base_length_bounds_are_exact() {
    assert_eq!(
        validate_api_base("", "github.example"),
        Err(GitHubClientError::Configuration(
            "the API base length is out of bounds"
        ))
    );
    let prefix = "https://github.example/";
    let at_limit = format!("{prefix}{}", "a".repeat(MAX_API_BASE_BYTES - prefix.len()));
    assert!(validate_api_base(&at_limit, "github.example").is_ok());
    let past_limit = format!(
        "{prefix}{}",
        "a".repeat(MAX_API_BASE_BYTES - prefix.len() + 1)
    );
    assert_eq!(
        validate_api_base(&past_limit, "github.example"),
        Err(GitHubClientError::Configuration(
            "the API base length is out of bounds"
        ))
    );
}

#[test]
fn verification_statuses_classify_facts_apart_from_failures() {
    let plain = reqwest::header::HeaderMap::new();
    assert_eq!(classified(200, &plain), Ok(Ok(())));
    assert_eq!(classified(404, &plain), Ok(Err(ForgeNegative::Missing)));
    assert_eq!(classified(422, &plain), Ok(Err(ForgeNegative::Missing)));
    assert_eq!(classified(403, &plain), Ok(Err(ForgeNegative::Denied)));
    let mut limited = reqwest::header::HeaderMap::new();
    limited.insert("retry-after", "30".parse().expect("a header value"));
    assert_eq!(classified(403, &limited), Err(ProviderError::Unavailable));
    assert_eq!(classified(429, &plain), Err(ProviderError::Unavailable));
    assert_eq!(classified(500, &plain), Err(ProviderError::Unavailable));
    assert_eq!(
        classified(401, &plain),
        Err(ProviderError::AuthorizationRevoked)
    );
    assert_eq!(classified(302, &plain), Err(ProviderError::InvalidResponse));
}
