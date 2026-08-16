#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed transport boundaries must fail loudly"
)]

use std::time::{Duration, Instant};

use amiss_controller::{ProviderError, ProviderInstance, ProviderNamespace};
use reqwest::StatusCode;
use secrecy::SecretString;

use super::{
    Budget, GitLabClientError, GitLabTimeouts, Transport, consume_bytes, map_error, map_status,
};

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
    assert_eq!(consume_bytes(budget, 6).unwrap().response_bytes, 4);
    assert!(matches!(
        consume_bytes(budget, 11),
        Err(ProviderError::InvalidResponse)
    ));
}

#[test]
fn status_mapping_is_fail_closed_and_debug_redacts_the_token() {
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        assert_eq!(map_status(status), ProviderError::AuthorizationRevoked);
    }
    for status in [
        StatusCode::REQUEST_TIMEOUT,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::BAD_GATEWAY,
    ] {
        assert_eq!(map_status(status), ProviderError::Unavailable);
    }
    assert_eq!(
        map_status(StatusCode::MOVED_PERMANENTLY),
        ProviderError::InvalidResponse
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

/// A budget that has run out refuses rather than handing back nothing.
#[test]
fn a_spent_budget_refuses_the_next_request() {
    let live = Budget {
        deadline: Instant::now() + Duration::from_secs(1),
        response_bytes: 10,
    };
    let remaining = live.remaining().expect("a live budget has time");
    assert!(!remaining.is_zero() && remaining <= Duration::from_secs(1));

    let spent = Budget {
        deadline: Instant::now(),
        response_bytes: 10,
    };
    assert_eq!(spent.remaining(), Err(ProviderError::Unavailable));
}

/// The transport reports the instance it was built for, and says what a
/// misconfiguration was.
#[test]
fn a_transport_names_its_instance_and_its_refusals() {
    let transport = Transport::new(
        provider(),
        "https://gitlab.example/api/v4",
        SecretString::from(TOKEN.to_owned()),
        GitLabTimeouts::default(),
    )
    .expect("a root-mounted https base");
    assert_eq!(transport.provider_instance(), "gitlab.example");

    assert_eq!(
        GitLabClientError("the API base must use https").to_string(),
        "the GitLab client configuration is invalid: the API base must use https"
    );
}

/// A request that never reached the server is unavailable; one that came back
/// wrong is an invalid response.
#[test]
fn transport_failures_separate_the_wire_from_the_answer() {
    let unreachable = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1))
        .build()
        .expect("a client")
        .get("https://127.0.0.1:1/never")
        .send()
        .expect_err("a closed port refuses");
    assert_eq!(map_error(&unreachable), ProviderError::Unavailable);

    let decode = reqwest::blocking::Client::new()
        .get("not-a-url")
        .build()
        .expect_err("a relative url cannot build");
    assert_eq!(map_error(&decode), ProviderError::InvalidResponse);
}
