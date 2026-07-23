#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed transport boundaries must fail loudly"
)]

use std::time::{Duration, Instant};

use amiss_controller::{ProviderError, ProviderInstance, ProviderNamespace};
use reqwest::StatusCode;
use secrecy::SecretString;

use super::{Budget, GitLabTimeouts, Transport, body_limit, consume_bytes, response_status};

const TOKEN: &str = "glpat-never-print-this";

#[test]
fn client_accepts_only_the_root_mounted_https_api() {
    for base in [
        "https://gitlab.example/api/v4",
        "https://gitlab.example/api/v4/",
    ] {
        assert!(
            Transport::new(
                provider(),
                base,
                SecretString::from(TOKEN.to_owned()),
                GitLabTimeouts::default(),
            )
            .is_ok()
        );
    }
    for base in [
        "http://gitlab.example/api/v4/",
        "https://other.example/api/v4/",
        "https://gitlab.example/gitlab/api/v4/",
        "https://user@gitlab.example/api/v4/",
        "https://gitlab.example:8443/api/v4/",
        "https://gitlab.example/api/v4/?x=1",
    ] {
        assert!(
            Transport::new(
                provider(),
                base,
                SecretString::from(TOKEN.to_owned()),
                GitLabTimeouts::default(),
            )
            .is_err()
        );
    }
}

#[test]
fn time_and_aggregate_body_budgets_are_bounded() {
    assert!(GitLabTimeouts::new(Duration::from_secs(10), Duration::from_mins(1), 1024).is_some());
    for invalid in [
        GitLabTimeouts::new(Duration::ZERO, Duration::from_mins(1), 1024),
        GitLabTimeouts::new(Duration::from_secs(31), Duration::from_mins(1), 1024),
        GitLabTimeouts::new(Duration::from_secs(10), Duration::ZERO, 1024),
        GitLabTimeouts::new(Duration::from_secs(10), Duration::from_secs(9), 1024),
        GitLabTimeouts::new(Duration::from_secs(10), Duration::from_secs(121), 1024),
        GitLabTimeouts::new(Duration::from_secs(10), Duration::from_mins(1), 0),
        GitLabTimeouts::new(
            Duration::from_secs(10),
            Duration::from_mins(1),
            8 * 1024 * 1024 + 1,
        ),
    ] {
        assert!(invalid.is_none());
    }

    let budget = Budget {
        deadline: Instant::now() + Duration::from_secs(1),
        response_bytes: 10,
    };
    assert_eq!(body_limit(Some(10), budget), Ok(11));
    assert_eq!(
        body_limit(Some(11), budget),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(consume_bytes(budget, 6).unwrap().response_bytes, 4);
    assert!(matches!(
        consume_bytes(budget, 11),
        Err(ProviderError::InvalidResponse)
    ));
}

#[test]
fn status_mapping_is_fail_closed_and_debug_redacts_the_token() {
    assert_eq!(response_status(StatusCode::OK), Ok(()));
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        assert_eq!(
            response_status(status),
            Err(ProviderError::AuthorizationRevoked)
        );
    }
    for status in [
        StatusCode::REQUEST_TIMEOUT,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::BAD_GATEWAY,
    ] {
        assert_eq!(response_status(status), Err(ProviderError::Unavailable));
    }
    assert_eq!(
        response_status(StatusCode::MOVED_PERMANENTLY),
        Err(ProviderError::InvalidResponse)
    );
    let transport = Transport::new(
        provider(),
        "https://gitlab.example/api/v4/",
        SecretString::from(TOKEN.to_owned()),
        GitLabTimeouts::default(),
    )
    .unwrap();
    let debug = format!("{transport:?}");
    assert!(!debug.contains(TOKEN));
    assert!(debug.contains("[REDACTED]"));
}

fn provider() -> amiss_controller::ProviderIdentity {
    amiss_controller::ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("gitlab.example".to_owned()).unwrap(),
    }
}
