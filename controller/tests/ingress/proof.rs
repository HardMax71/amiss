use std::time::Duration;

use amiss_controller::{DeliveryHeader, IngressError, SignedTimePolicy};

use super::support::{
    BODY, FixedClock, GITHUB_HEADERS, GITLAB_BODY, GITLAB_HEADERS, GITLAB_NOW, delivery,
    github_proof, github_verified, gitlab_verified, opaque, policy, provider, raw, route,
};

#[test]
fn route_and_trust_set_must_match_after_authentication() {
    let route = route(SignedTimePolicy::ReplayOnly);
    let policy = policy(Duration::from_secs(1), Duration::ZERO);
    let check = policy
        .pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_000)),
        )
        .unwrap();
    let wrong_trust = github_verified(check, &route.provider, opaque("other-keyring"));
    assert_eq!(
        policy.post_auth(check, wrong_trust),
        Err(IngressError::Route)
    );

    let wrong_provider = github_verified(
        check,
        &provider("other.example.test"),
        route.trust_set.clone(),
    );
    assert_eq!(
        policy.post_auth(check, wrong_provider),
        Err(IngressError::Route)
    );
}

#[test]
fn proof_is_bound_to_every_raw_request_byte() {
    let route = route(SignedTimePolicy::ReplayOnly);
    let policy = policy(Duration::from_secs(1), Duration::ZERO);
    let signed = policy
        .pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_000)),
        )
        .unwrap();
    let proof = github_verified(signed, &route.provider, route.trust_set.clone());

    let changed_body = policy
        .pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, b"{}"),
            &FixedClock(Some(1_000)),
        )
        .unwrap();
    assert_eq!(
        policy.post_auth(changed_body, proof.clone()),
        Err(IngressError::Request)
    );

    let changed_headers = [DeliveryHeader {
        name: "x-hub-signature-256",
        value: b"sha256=0000000000000000000000000000000000000000000000000000000000000000",
    }];
    let changed_header = policy
        .pre_auth(
            raw(&route, 1_000, &changed_headers, BODY),
            &FixedClock(Some(1_000)),
        )
        .unwrap();
    assert_eq!(
        policy.post_auth(changed_header, proof),
        Err(IngressError::Request)
    );

    let changed_receipt = policy
        .pre_auth(
            raw(&route, 1_001, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_001)),
        )
        .unwrap();
    let original = policy
        .pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_000)),
        )
        .unwrap();
    let proof = github_verified(original, &route.provider, route.trust_set.clone());
    assert_eq!(
        policy.post_auth(changed_receipt, proof),
        Err(IngressError::Request)
    );
}

#[test]
fn identical_bytes_on_another_route_do_not_share_a_proof() {
    let route_a = route(SignedTimePolicy::ReplayOnly);
    let mut route_b = route(SignedTimePolicy::ReplayOnly);
    route_b.provider = provider("other.example.test");
    let policy = policy(Duration::from_secs(1), Duration::ZERO);
    let check_a = policy
        .pre_auth(
            raw(&route_a, 1_000, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_000)),
        )
        .unwrap();
    let proof_a = github_verified(check_a, &route_b.provider, route_b.trust_set.clone());
    let check_b = policy
        .pre_auth(
            raw(&route_b, 1_000, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_000)),
        )
        .unwrap();

    assert_eq!(
        policy.post_auth(check_b, proof_a),
        Err(IngressError::Request)
    );
}

#[test]
fn replay_identity_is_normalized_only_after_verification() {
    let signed_route = route(SignedTimePolicy::Required(Duration::from_secs(100)));
    let policy = policy(Duration::from_secs(200), Duration::from_secs(10));
    let signed_check = policy
        .pre_auth(
            raw(&signed_route, GITLAB_NOW, GITLAB_HEADERS, GITLAB_BODY),
            &FixedClock(Some(GITLAB_NOW)),
        )
        .unwrap();
    let signed = policy
        .post_auth(
            signed_check,
            gitlab_verified(signed_check, &signed_route.provider),
        )
        .unwrap();
    assert_eq!(
        signed.identity.delivery.as_str(),
        "f5e5f430-f57b-4e6e-9fac-d9128cd7232f"
    );

    let exact_route = route(SignedTimePolicy::ReplayOnly);
    let exact_check = policy
        .pre_auth(
            raw(&exact_route, GITLAB_NOW, GITHUB_HEADERS, BODY),
            &FixedClock(Some(GITLAB_NOW)),
        )
        .unwrap();
    let exact = policy
        .post_auth(
            exact_check,
            github_verified(
                exact_check,
                &exact_route.provider,
                exact_route.trust_set.clone(),
            ),
        )
        .unwrap();
    assert_eq!(
        exact.identity.delivery.as_str(),
        "body:sha256:70625f14c886c25b874c1bf13658987108dd149896764fc6707b06164e83a233"
    );
}

#[test]
fn debug_output_never_contains_raw_credentials_or_body() {
    let route = route(SignedTimePolicy::ReplayOnly);
    let secret = b"legacy-credential-must-not-appear";
    let body = b"body-secret-must-not-appear";
    let headers = [DeliveryHeader {
        name: "x-gitlab-token",
        value: secret,
    }];
    let request = raw(&route, 1_000, &headers, body);
    let checked = policy(Duration::from_secs(1), Duration::ZERO)
        .pre_auth(request, &FixedClock(Some(1_000)))
        .unwrap();
    let rendered = format!("{request:?} {checked:?} {:?}", headers[0]);

    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("legacy-credential"));
    assert!(!rendered.contains("body-secret"));

    let valid = policy(Duration::from_secs(1), Duration::ZERO)
        .pre_auth(
            raw(&route, 1_000, GITHUB_HEADERS, BODY),
            &FixedClock(Some(1_000)),
        )
        .unwrap();
    let proof = github_proof(valid, route.trust_set.clone());
    let proof_rendered = format!("{proof:?}");
    let verified_rendered = format!("{:?}", proof.bind(delivery(&route.provider)));
    let combined = format!("{proof_rendered} {verified_rendered}");
    assert!(!combined.contains("event"));
    assert!(!combined.contains("ac6a6901"));
}
