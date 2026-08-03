#![expect(
    clippy::unwrap_used,
    reason = "fixed cryptographic fixtures and protocol identities must fail loudly"
)]

mod support;

use amiss_controller::{
    OpaqueId, ProviderError, ProviderInstance, ReplayIdentity, SignedTimePolicy,
};
use serde_json::{Value, json};

use support::identity::now_seconds;
use support::oidc::{
    accept, claims, oidc, route, set_claim, sign, verify, verify_routed, verify_signed,
};

const BODY: &[u8] = br#"{"merge_request_iid":42}"#;

#[test]
fn pinned_policy_job_claims_define_the_delivery() {
    let now = now_seconds();
    let source = oidc();
    let accepted = accept(&source, &claims(now), BODY, now).unwrap();
    let delivery = accepted.delivery();

    assert_eq!(delivery.identity.integration.as_str(), "policy/1");
    assert_eq!(delivery.change.repository.owner, "acme");
    assert_eq!(delivery.change.repository.name, "widget");
    assert_eq!(
        delivery.change.change.as_str(),
        "project/101/merge-request/42"
    );
    assert_eq!(
        delivery.provider_run.run_id.as_str(),
        "pipeline/202/job/303"
    );
    assert_eq!(
        delivery.provider_run.candidate_commit.as_str(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert!(
        delivery
            .identity
            .delivery
            .as_str()
            .starts_with("oidc/runner/77/jti/")
    );
    assert!(!delivery.identity.delivery.as_str().contains("2d7d0a3f"));
    assert!(matches!(
        verify(&source, &claims(now), BODY, now).unwrap().replay(),
        ReplayIdentity::Authenticated(_)
    ));
}

#[test]
fn issuer_audience_policy_project_and_run_claims_are_exact() {
    let now = now_seconds();
    let source = oidc();
    let cases = [
        changed(now, "iss", json!("https://attacker.invalid")),
        changed(now, "aud", json!("other-controller")),
        changed(now, "job_project_id", json!("102")),
        changed(now, "job_project_path", json!("acme/other")),
        changed(now, "pipeline_id", json!("0")),
        changed(now, "pipeline_source", json!("push")),
        changed(now, "job_id", json!("0")),
        changed(now, "job_source", json!("project")),
        changed(now, "sha", json!("not-an-oid")),
    ];
    for case in cases {
        assert_eq!(
            verify(&source, &case, BODY, now),
            Err(ProviderError::Authentication)
        );
    }

    let mut wrong_url = claims(now);
    *wrong_url
        .get_mut("job_config")
        .unwrap()
        .get_mut("url")
        .unwrap() = json!("https://gitlab.example/project/.gitlab-ci.yml");
    let mut wrong_sha = claims(now);
    *wrong_sha
        .get_mut("job_config")
        .unwrap()
        .get_mut("sha")
        .unwrap() = json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    for case in [wrong_url, wrong_sha] {
        assert_eq!(
            verify(&source, &case, BODY, now),
            Err(ProviderError::Authentication)
        );
    }
}

#[test]
fn runner_jti_time_and_request_hint_fail_closed() {
    let now = now_seconds();
    let source = oidc();
    let cases = [
        changed(now, "runner_id", json!("0")),
        changed(now, "runner_environment", json!("project")),
        changed(now, "jti", json!("")),
        changed(now, "jti", json!("x".repeat(1_025))),
        changed(now, "jti", json!("line\nbreak")),
        changed(now, "aud", json!("")),
        changed(now, "sub", json!("")),
        changed(now, "iat", json!(now.saturating_add(301))),
        changed(now, "nbf", json!(now.saturating_add(301))),
    ];
    for case in cases {
        assert_eq!(
            verify(&source, &case, BODY, now),
            Err(ProviderError::Authentication)
        );
    }

    let mut wrong_self_hosted = claims(now);
    set_claim(
        &mut wrong_self_hosted,
        "runner_environment",
        json!("self-hosted"),
    );
    set_claim(&mut wrong_self_hosted, "runner_id", json!("88"));
    assert_eq!(
        verify(&source, &wrong_self_hosted, BODY, now),
        Err(ProviderError::Authentication)
    );
    let mut self_hosted = claims(now);
    set_claim(&mut self_hosted, "runner_environment", json!("self-hosted"));
    assert!(accept(&source, &self_hosted, BODY, now).is_ok());
    for body in [
        br#"{"merge_request_iid":0}"#.as_slice(),
        br#"{"merge_request_iid":42,"project_id":101}"#.as_slice(),
        br"{}".as_slice(),
    ] {
        assert_eq!(
            verify(&source, &claims(now), body, now),
            Err(ProviderError::Authentication)
        );
    }
}

#[test]
fn signature_headers_and_freshness_are_not_advisory() {
    let now = now_seconds();
    let source = oidc();
    let token = sign(&claims(now));
    assert_eq!(
        verify_signed(&source, &token, BODY, now, true),
        Err(ProviderError::Authentication)
    );
    let mut tampered = token.into_bytes();
    let last = tampered.last_mut().unwrap();
    *last = if *last == b'a' { b'b' } else { b'a' };
    assert_eq!(
        verify_signed(
            &source,
            std::str::from_utf8(&tampered).unwrap(),
            BODY,
            now,
            false
        ),
        Err(ProviderError::Authentication)
    );

    let mut stale = claims(now);
    set_claim(&mut stale, "iat", json!(now - 600));
    set_claim(&mut stale, "nbf", json!(now - 601));
    assert!(accept(&source, &stale, BODY, now).is_err());

    let mut expired = claims(now);
    set_claim(&mut expired, "iat", json!(now - 20));
    set_claim(&mut expired, "nbf", json!(now - 21));
    set_claim(&mut expired, "exp", json!(now - 5));
    assert_eq!(
        verify(&source, &expired, BODY, now),
        Err(ProviderError::Authentication)
    );
}

fn changed(now: u64, name: &str, value: Value) -> Value {
    let mut changed = claims(now);
    set_claim(&mut changed, name, value);
    changed
}

#[test]
fn one_trusted_runner_source_is_enough() {
    use support::oidc::{KID, accepts, audience, issuer_url, keys_with, policy_binding};

    let mut hosted_only = policy_binding();
    hosted_only.runners.self_hosted_ids.clear();
    assert!(
        accepts(&issuer_url(), &audience(), hosted_only, keys_with(KID)),
        "gitlab-hosted runners alone are a trusted source"
    );

    let mut pinned_only = policy_binding();
    pinned_only.runners.gitlab_hosted = false;
    assert!(
        accepts(&issuer_url(), &audience(), pinned_only, keys_with(KID)),
        "pinned self-hosted runners alone are a trusted source"
    );
}

#[test]
fn the_key_identifier_grammar_is_exact() {
    use support::oidc::try_key;

    assert!(try_key(&"k".repeat(256)).is_ok(), "the longest legal kid");
    let overlong = "k".repeat(257);
    for (kid, reason) in [
        ("", "empty"),
        (overlong.as_str(), "overlong"),
        ("ki\"d", "a quote"),
        ("ki\\d", "a backslash"),
        ("ki d", "a space"),
        ("kid\u{7f}", "a control byte"),
    ] {
        assert!(try_key(kid).is_err(), "{reason} is refused");
    }
}

const RSA_N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";

fn jwks(kids: &[String]) -> jsonwebtoken::jwk::JwkSet {
    let keys = kids
        .iter()
        .map(|kid| json!({"kty": "RSA", "kid": kid, "n": RSA_N, "e": "AQAB"}))
        .collect::<Vec<_>>();
    serde_json::from_value(json!({ "keys": keys })).unwrap()
}

fn anchors(kids: &[String]) -> std::collections::BTreeMap<String, amiss_controller::TrustAnchorId> {
    use amiss_controller::OpaqueId;
    kids.iter()
        .map(|kid| {
            (
                kid.clone(),
                OpaqueId::new(format!("gitlab-key/{kid}")).unwrap(),
            )
        })
        .collect()
}

fn numbered(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("kid-{index}")).collect()
}

#[test]
fn a_jwks_converts_exactly_within_its_ceiling() {
    use amiss_controller_gitlab::{MAX_KEYS, public_keys_from_jwks};

    let one = numbered(1);
    let keys = public_keys_from_jwks(&jwks(&one), &anchors(&one)).unwrap();
    assert_eq!(keys.len(), 1, "one pinned key comes back as one key");

    let exact = numbered(MAX_KEYS);
    assert_eq!(
        public_keys_from_jwks(&jwks(&exact), &anchors(&exact))
            .unwrap()
            .len(),
        MAX_KEYS,
        "a set exactly at the ceiling converts whole"
    );

    let over = numbered(MAX_KEYS + 1);
    assert!(
        public_keys_from_jwks(&jwks(&over), &anchors(&over)).is_err(),
        "one key past the ceiling is refused"
    );

    let none: Vec<String> = Vec::new();
    assert!(
        public_keys_from_jwks(&jwks(&none), &anchors(&none)).is_err(),
        "an empty set is refused"
    );
}

#[test]
fn a_jwks_that_disagrees_with_its_anchors_is_refused() {
    use amiss_controller_gitlab::public_keys_from_jwks;

    let one = numbered(1);
    let two = numbered(2);
    assert!(
        public_keys_from_jwks(&jwks(&one), &anchors(&two)).is_err(),
        "an anchor count that differs from the key count"
    );
    assert!(
        public_keys_from_jwks(&jwks(&one), &anchors(&["other".to_owned()])).is_err(),
        "an anchor set that names a different kid"
    );

    let duplicated = vec!["kid-0".to_owned(), "kid-0".to_owned()];
    assert!(
        public_keys_from_jwks(&jwks(&duplicated), &anchors(&two)).is_err(),
        "a duplicated kid is refused even when the counts agree"
    );

    let elliptic: jsonwebtoken::jwk::JwkSet = serde_json::from_value(json!({"keys": [{
        "kty": "EC",
        "kid": "kid-0",
        "crv": "P-256",
        "x": "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU",
        "y": "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0"
    }]}))
    .unwrap();
    assert!(
        public_keys_from_jwks(&elliptic, &anchors(&one)).is_err(),
        "a key outside the RSA family is refused"
    );

    let anonymous: jsonwebtoken::jwk::JwkSet =
        serde_json::from_value(json!({"keys": [{"kty": "RSA", "n": RSA_N, "e": "AQAB"}]})).unwrap();
    assert!(
        public_keys_from_jwks(&anonymous, &anchors(&one)).is_err(),
        "a key without an identifier is refused"
    );
}

#[test]
fn the_config_error_names_itself() {
    use amiss_controller_gitlab::public_keys_from_jwks;

    let none: Vec<String> = Vec::new();
    let error = public_keys_from_jwks(&jwks(&none), &anchors(&none)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "the GitLab OIDC configuration is invalid"
    );
}

#[test]
fn every_clause_binding_the_policy_is_load_bearing() {
    use amiss_controller_gitlab::PolicyBinding;
    use support::oidc::{KID, accepts, audience, issuer_url, keys_with, policy_binding};
    let valid = |policy: PolicyBinding| accepts(&issuer_url(), &audience(), policy, keys_with(KID));
    assert!(valid(policy_binding()), "the fixture itself is accepted");

    let refused = |name: &str, break_it: &dyn Fn(PolicyBinding) -> PolicyBinding| {
        assert!(!valid(break_it(policy_binding())), "{name} is refused");
    };
    refused("zero project id", &|mut p: PolicyBinding| {
        p.project_id = 0;
        p
    });
    refused("path is not canonical", &|mut p: PolicyBinding| {
        p.project_path = format!("/{}/", p.project_path);
        p
    });
    refused("empty branch", &|mut p: PolicyBinding| {
        p.target_branch = String::new();
        p
    });
    refused("overlong branch", &|mut p: PolicyBinding| {
        p.target_branch = "b".repeat(256);
        p
    });
    refused("empty job", &|mut p: PolicyBinding| {
        p.job_name = String::new();
        p
    });
    refused("overlong job", &|mut p: PolicyBinding| {
        p.job_name = "j".repeat(256);
        p
    });
    refused("config url off host", &|mut p: PolicyBinding| {
        p.config_url = "https://elsewhere.example/policy.yml".to_owned();
        p
    });
    refused("no trusted runner", &|mut p: PolicyBinding| {
        p.runners.gitlab_hosted = false;
        p.runners.self_hosted_ids.clear();
        p
    });

    for issuer in [
        "http://gitlab.example",
        "https://elsewhere.example",
        "https://gitlab.example:8443",
        "https://user@gitlab.example",
        "https://gitlab.example/?a=b",
        "https://gitlab.example/#f",
    ] {
        assert!(
            !accepts(issuer, &audience(), policy_binding(), keys_with(KID)),
            "{issuer} is refused"
        );
    }

    assert!(
        !accepts(&issuer_url(), "", policy_binding(), keys_with(KID)),
        "an empty audience is refused"
    );
    assert!(
        !accepts(
            &issuer_url(),
            &"a".repeat(2_049),
            policy_binding(),
            keys_with(KID)
        ),
        "an overlong audience is refused"
    );
    assert!(
        !accepts(&issuer_url(), &audience(), policy_binding(), Vec::new()),
        "a keyless config is refused"
    );
}

/// The bounds are inclusive: a token issued at its own expiry, valid from it,
/// and carrying an identifier of exactly the ceiling length still verifies.
#[test]
fn a_token_at_its_own_bounds_is_still_authentic() {
    let now = now_seconds();
    let source = oidc();

    let mut at_jti_ceiling = claims(now);
    set_claim(&mut at_jti_ceiling, "jti", json!("x".repeat(1_024)));
    assert!(
        accept(&source, &at_jti_ceiling, BODY, now).is_ok(),
        "an identifier exactly at its ceiling"
    );

    // The expiry comes down to meet the issue time, since an issue time in the
    // future is refused for freshness before these claims are compared.
    let mut at_expiry = claims(now);
    set_claim(&mut at_expiry, "exp", json!(now));
    assert!(
        accept(&source, &at_expiry, BODY, now).is_ok(),
        "issued at its own expiry"
    );

    let mut valid_from_expiry = claims(now);
    set_claim(&mut valid_from_expiry, "exp", json!(now));
    set_claim(&mut valid_from_expiry, "nbf", json!(now));
    assert!(
        accept(&source, &valid_from_expiry, BODY, now).is_ok(),
        "valid from its own expiry"
    );
}

/// GitLab spells its identifiers as strings, but a number is the same
/// identifier and the run must read it as one.
#[test]
fn a_numeric_identifier_is_the_same_identifier() {
    let now = now_seconds();
    let source = oidc();
    let mut numeric = claims(now);
    set_claim(&mut numeric, "runner_id", json!(77));
    set_claim(&mut numeric, "pipeline_id", json!(202));
    set_claim(&mut numeric, "job_id", json!(303));
    let delivery = accept(&source, &numeric, BODY, now).expect("a numeric identifier verifies");
    assert!(
        delivery
            .delivery()
            .identity
            .delivery
            .as_str()
            .starts_with("oidc/runner/77/jti/"),
        "the number it read is the runner it names"
    );
}

/// The route contract is three separate clauses: the provider, the trust set,
/// and a signed-time policy that actually demands signed time. Each is broken
/// alone, since a run that passes two of three is not routed here.
#[test]
fn every_route_clause_stands_alone() {
    let now = now_seconds();
    let source = oidc();
    assert!(verify_routed(&source, &route(), now).is_ok());

    let mut other_provider = route();
    other_provider.provider.instance = ProviderInstance::new("other.example".to_owned()).unwrap();
    let mut other_trust_set = route();
    other_trust_set.trust_set = OpaqueId::new("gitlab-webhook".to_owned()).unwrap();
    let mut replay_only = route();
    replay_only.signed_time = SignedTimePolicy::ReplayOnly;

    for broken in [other_provider, other_trust_set, replay_only] {
        assert_eq!(
            verify_routed(&source, &broken, now),
            Err(ProviderError::Authentication),
            "{broken:?}"
        );
    }
}

/// A token past the header ceiling is refused for its length, whatever it
/// would have proved: the bearer grammar runs before any signature does.
#[test]
fn a_token_past_the_ceiling_is_refused_before_it_is_verified() {
    let now = now_seconds();
    let source = oidc();
    let mut padded = claims(now);
    padded.as_object_mut().unwrap().insert(
        "padding".to_owned(),
        serde_json::json!("p".repeat(16 * 1024)),
    );
    let oversized = sign(&padded);
    assert!(
        oversized.len() > 16 * 1024,
        "the fixture has to cross the ceiling: {}",
        oversized.len()
    );
    assert_eq!(
        verify_signed(&source, &oversized, BODY, now, false),
        Err(ProviderError::Authentication)
    );
}

/// The source prints what it is without printing what it holds.
#[test]
fn the_source_names_itself_without_its_keys() {
    let printed = format!("{:?}", oidc());
    assert!(printed.contains("GitLabOidc"), "{printed}");
    assert!(printed.contains("gitlab.example"), "{printed}");
    assert!(printed.contains("current"), "the key identifier: {printed}");
}
