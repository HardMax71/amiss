#![cfg(test)]

use std::time::Duration;

use amiss_controller::{ForgeNegative, ProviderError};
use reqwest::StatusCode;
use secrecy::SecretString;

use super::super::super::GiteaTimeouts;
use super::{MAX_API_BASE_BYTES, MAX_RESPONSE_BYTES, Transport, map_status, validate_api_base};

#[test]
fn the_ceilings_are_the_documented_values() {
    assert_eq!(MAX_RESPONSE_BYTES, 8_388_608);
    assert_eq!(MAX_API_BASE_BYTES, 2_048);
}

#[test]
fn the_status_table_is_exact() {
    let of = |code: u16| map_status(StatusCode::from_u16(code).unwrap());
    assert_eq!(of(401), ProviderError::AuthorizationRevoked);
    assert_eq!(of(403), ProviderError::AuthorizationRevoked);
    assert_eq!(of(408), ProviderError::Unavailable);
    assert_eq!(of(425), ProviderError::Unavailable);
    assert_eq!(of(429), ProviderError::Unavailable);
    assert_eq!(of(500), ProviderError::Unavailable);
    assert_eq!(of(503), ProviderError::Unavailable);
    assert_eq!(of(404), ProviderError::InvalidResponse);
    assert_eq!(of(499), ProviderError::InvalidResponse);
}

#[test]
fn a_route_is_one_absolute_unrepeated_path() {
    let timeouts = GiteaTimeouts::new(Duration::from_secs(1), Duration::from_secs(3)).unwrap();
    let transport = Transport::new(
        "forge.example",
        "https://forge.example/api/v1",
        SecretString::from("a-secure-dedicated-token"),
        timeouts,
    )
    .unwrap();
    assert_eq!(
        transport.url("/repos/acme/widget").unwrap().as_str(),
        "https://forge.example/api/v1/repos/acme/widget"
    );
    assert!(
        transport.url("repos").is_err(),
        "a relative route is refused"
    );
    assert!(
        transport.url("//evil.example").is_err(),
        "a protocol-relative route is refused"
    );
}

#[test]
fn the_api_base_grammar_names_each_refusal() {
    let expect = |raw: &str, text: &'static str| {
        assert_eq!(
            validate_api_base(raw, "forge.example").unwrap_err(),
            super::super::super::GiteaClientError::Configuration(text),
            "{raw:.60}"
        );
    };
    expect("", "the API base length is out of bounds");
    let over = format!("https://forge.example/api/v1{}", "x".repeat(2_021));
    assert_eq!(over.len(), MAX_API_BASE_BYTES + 1);
    expect(&over, "the API base length is out of bounds");
    let exact = format!("https://forge.example/api/v1{}", "x".repeat(2_020));
    assert_eq!(exact.len(), MAX_API_BASE_BYTES);
    expect(&exact, "the API base must mount /api/v1 at the root");
    expect(
        "https://:secret@forge.example/api/v1",
        "the API base must not carry credentials",
    );
    expect(
        "https://forge.example/api/v1#f",
        "the API base must not carry a query or fragment",
    );
}

#[test]
fn a_review_page_is_complete_exactly_under_its_size() {
    use super::super::{PAGE_SIZE, page_complete};

    assert_eq!(page_complete(0), Ok(true));
    assert_eq!(page_complete(PAGE_SIZE - 1), Ok(true));
    assert_eq!(page_complete(PAGE_SIZE), Ok(false));
    assert_eq!(
        page_complete(PAGE_SIZE + 1),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn a_repository_route_names_its_owner_and_repository() {
    use amiss_controller::{
        ChangeId, ChangeLocator, ProviderIdentity, ProviderInstance, ProviderNamespace,
    };
    use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

    let change = ChangeLocator {
        provider: ProviderIdentity {
            namespace: ProviderNamespace::new("gitea".to_owned()).unwrap(),
            instance: ProviderInstance::new("forge.example".to_owned()).unwrap(),
        },
        repository: RepositoryIdentity::new(
            "forge.example".to_owned(),
            "acme".to_owned(),
            "widget".to_owned(),
        )
        .unwrap(),
        change: ChangeId::new("repository/101/pull/4201/number/42".to_owned()).unwrap(),
    };
    let candidate = Oid::new(ObjectFormat::Sha1, "b".repeat(40)).unwrap();
    let pull_request = crate::GiteaPullRequest {
        change: &change,
        reviewer_id: 77,
        repository_id: 101,
        repository_owner: "acme",
        repository_name: "widget",
        pull_request_id: 4201,
        number: 42,
        candidate_commit: &candidate,
    };
    let route =
        super::super::repository_route(pull_request.repository_owner, pull_request.repository_name);
    assert_eq!(route, "/repos/acme/widget");
}

#[test]
fn a_deadline_keeps_a_positive_remainder_or_refuses() {
    use super::super::OperationDeadline;

    let open = OperationDeadline::after(Duration::from_mins(1)).unwrap();
    assert!(
        open.remaining().unwrap() > Duration::ZERO,
        "a fresh deadline has time left"
    );
    let spent = OperationDeadline::after(Duration::ZERO).unwrap();
    assert_eq!(spent.remaining(), Err(ProviderError::Unavailable));
}

#[test]
fn verification_statuses_classify_facts_apart_from_failures() {
    let of = |code: u16| super::classified(StatusCode::from_u16(code).unwrap());
    assert_eq!(of(200), Some(Ok(())));
    assert_eq!(of(404), Some(Err(ForgeNegative::Missing)));
    assert_eq!(of(422), Some(Err(ForgeNegative::Missing)));
    assert_eq!(of(403), Some(Err(ForgeNegative::Denied)));
    assert_eq!(of(429), None);
    assert_eq!(of(500), None);
    assert_eq!(of(401), None);
    assert_eq!(of(302), None);
}
