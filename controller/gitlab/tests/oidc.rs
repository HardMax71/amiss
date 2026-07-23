#![expect(
    clippy::unwrap_used,
    reason = "fixed cryptographic fixtures and protocol identities must fail loudly"
)]

mod support;

use amiss_controller::{ProviderError, ReplayIdentity};
use serde_json::{Value, json};

use support::identity::now_seconds;
use support::oidc::{accept, claims, oidc, set_claim, sign, verify, verify_signed};

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
