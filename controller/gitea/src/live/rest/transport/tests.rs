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
fn native_client_configuration_rejects_invalid_tokens() {
    let timeouts = GiteaTimeouts::new(Duration::from_secs(1), Duration::from_secs(3)).unwrap();
    for invalid in ["token\r\nAccept: text/plain", "token\0suffix"] {
        assert_eq!(
            Transport::new(
                "forge.example",
                "https://forge.example/api/v1",
                SecretString::from(invalid),
                timeouts,
            )
            .err(),
            Some(super::super::super::GiteaClientError::Configuration(
                "the token is not a valid HTTP header"
            ))
        );
    }
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
            namespace: ProviderNamespace::try_from("gitea".to_owned()).unwrap(),
            instance: ProviderInstance::try_from("forge.example".to_owned()).unwrap(),
        },
        repository: RepositoryIdentity::new(
            "forge.example".to_owned(),
            "acme".to_owned(),
            "widget".to_owned(),
        )
        .unwrap(),
        change: ChangeId::try_from("repository/101/pull/4201/number/42".to_owned()).unwrap(),
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
    let of = |code: u16| {
        let response = http::Response::builder()
            .status(code)
            .body("not JSON")
            .unwrap();
        super::classified(response.into()).map(|fact| fact.map(|_response| ()))
    };
    assert_eq!(of(200), Ok(Ok(())));
    assert_eq!(of(404), Ok(Err(ForgeNegative::Missing)));
    assert_eq!(of(422), Ok(Err(ForgeNegative::Missing)));
    assert_eq!(of(403), Ok(Err(ForgeNegative::Denied)));
    assert_eq!(of(429), Err(ProviderError::Unavailable));
    assert_eq!(of(500), Err(ProviderError::Unavailable));
    assert_eq!(of(401), Err(ProviderError::AuthorizationRevoked));
    assert_eq!(of(302), Err(ProviderError::InvalidResponse));
}

#[test]
fn data_responses_classify_http_errors_before_decoding() {
    for code in [401, 403, 404, 408, 422, 429, 500, 503] {
        for body in ["[]", "not JSON"] {
            let response = http::Response::builder().status(code).body(body).unwrap();
            assert_eq!(
                super::decode_body::<Vec<crate::reference::RefRecord>>(response.into()),
                Err(map_status(StatusCode::from_u16(code).unwrap()))
            );
        }
    }
}

#[test]
fn verification_status_does_not_require_or_decode_a_response_body() {
    for body in [
        include_bytes!("../../../../tests/fixtures/gitea-file.json").as_slice(),
        include_bytes!("../../../../tests/fixtures/forgejo-file.json").as_slice(),
        include_bytes!("../../../../tests/fixtures/gitea-directory.json").as_slice(),
        include_bytes!("../../../../tests/fixtures/forgejo-directory.json").as_slice(),
        b"not JSON",
        b"",
    ] {
        let response = http::Response::builder()
            .status(200)
            .header(http::header::CONTENT_LENGTH, u64::MAX)
            .body(body)
            .unwrap();
        assert!(super::classified(response.into()).unwrap().is_ok());
    }
    let response = http::Response::new(vec![b' '; MAX_RESPONSE_BYTES + 1]);
    assert!(super::classified(response.into()).unwrap().is_ok());
}
