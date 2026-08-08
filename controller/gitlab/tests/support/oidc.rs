use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use amiss_controller::{
    AcceptedDelivery, DeliveryHeader, DeliveryRoute, IngressLimits, IngressPolicy, OpaqueId,
    ProviderError, ReplayWindow, SignedTimePolicy, UntrustedDelivery, VerifiedDelivery,
};
use amiss_controller_fixtures::{RsaKeys, pinned_rsa_keys};
use amiss_controller_gitlab::{
    GitLabConfigError, GitLabOidc, OidcPublicKey, PolicyBinding, RunnerTrust,
};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};

use super::identity::{HOST, PROJECT_PATH, TestClock, oid, provider};

const AUDIENCE: &str = "amiss-controller";
pub const KID: &str = "current";

static RSA_KEYS: LazyLock<RsaKeys> = LazyLock::new(pinned_rsa_keys);

pub fn oidc() -> Arc<GitLabOidc> {
    let policy = PolicyBinding {
        integration: OpaqueId::new("policy/1".to_owned()).unwrap(),
        project_id: 101,
        project_path: PROJECT_PATH.to_owned(),
        target_branch: "main".to_owned(),
        job_name: "amiss:policy".to_owned(),
        config_url: format!("https://{HOST}/security/policy.yml"),
        config_commit: oid('f'),
        runners: RunnerTrust {
            gitlab_hosted: true,
            self_hosted_ids: BTreeSet::from([77]),
        },
    };
    Arc::new(
        GitLabOidc::new(
            provider(),
            OpaqueId::new("gitlab-oidc".to_owned()).unwrap(),
            format!("https://{HOST}"),
            AUDIENCE.to_owned(),
            policy,
            vec![public_key()],
            2,
        )
        .unwrap(),
    )
}

pub fn policy_binding() -> PolicyBinding {
    PolicyBinding {
        integration: OpaqueId::new("policy/1".to_owned()).unwrap(),
        project_id: 101,
        project_path: PROJECT_PATH.to_owned(),
        target_branch: "main".to_owned(),
        job_name: "amiss:policy".to_owned(),
        config_url: format!("https://{HOST}/security/policy.yml"),
        config_commit: oid('f'),
        runners: RunnerTrust {
            gitlab_hosted: true,
            self_hosted_ids: BTreeSet::from([77]),
        },
    }
}

pub fn issuer_url() -> String {
    format!("https://{HOST}")
}

pub fn audience() -> String {
    AUDIENCE.to_owned()
}

pub fn keys_with(kid: &str) -> Vec<OidcPublicKey> {
    vec![try_key(kid).unwrap()]
}

pub fn try_key(kid: &str) -> Result<OidcPublicKey, GitLabConfigError> {
    OidcPublicKey::from_rsa_pem(
        kid.to_owned(),
        OpaqueId::new("gitlab-key/current".to_owned()).unwrap(),
        &RSA_KEYS.public_pem,
    )
}

pub fn accepts(
    issuer: &str,
    audience: &str,
    policy: PolicyBinding,
    keys: Vec<OidcPublicKey>,
) -> bool {
    GitLabOidc::new(
        provider(),
        OpaqueId::new("gitlab-oidc".to_owned()).unwrap(),
        issuer.to_owned(),
        audience.to_owned(),
        policy,
        keys,
        2,
    )
    .is_ok()
}

fn public_key() -> OidcPublicKey {
    OidcPublicKey::from_rsa_pem(
        KID.to_owned(),
        OpaqueId::new("gitlab-key/current".to_owned()).unwrap(),
        &RSA_KEYS.public_pem,
    )
    .unwrap()
}

pub fn claims(now: u64) -> Value {
    json!({
        "iss": format!("https://{HOST}"),
        "sub": "project_path:acme/widget:ref_type:branch:ref:topic",
        "aud": AUDIENCE,
        "exp": now.checked_add(300).unwrap(),
        "nbf": now.checked_sub(1).unwrap(),
        "iat": now,
        "jti": "2d7d0a3f-4aaf-47f5-aeec-291a7c40eef0",
        "job_project_id": "101",
        "job_project_path": PROJECT_PATH,
        "pipeline_id": "202",
        "pipeline_source": "merge_request_event",
        "job_id": "303",
        "runner_id": "77",
        "runner_environment": "gitlab-hosted",
        "sha": oid('b').as_str(),
        "job_source": "pipeline_execution_policy",
        "job_config": {
            "url": format!("https://{HOST}/security/policy.yml"),
            "sha": oid('f').as_str()
        }
    })
}

pub fn set_claim(claims: &mut Value, name: &str, value: Value) {
    *claims.get_mut(name).unwrap() = value;
}

pub fn sign(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_owned());
    encode(
        &header,
        claims,
        &EncodingKey::from_rsa_pem(&RSA_KEYS.private_pem).unwrap(),
    )
    .unwrap()
}

pub fn verify(
    source: &GitLabOidc,
    claims: &Value,
    body: &[u8],
    now: u64,
) -> Result<VerifiedDelivery, ProviderError> {
    let token = sign(claims);
    verify_signed(source, &token, body, now, false)
}

pub fn verify_signed(
    source: &GitLabOidc,
    token: &str,
    body: &[u8],
    now: u64,
    duplicate_header: bool,
) -> Result<VerifiedDelivery, ProviderError> {
    verify_token(source, token, body, now, duplicate_header)
}

/// Authenticates the standard signed delivery over a caller-chosen route, so
/// one clause of the route contract can be broken at a time.
pub fn verify_routed(
    source: &GitLabOidc,
    route: &DeliveryRoute,
    now: u64,
) -> Result<VerifiedDelivery, ProviderError> {
    let token = sign(&claims(now));
    let authorization = format!("Bearer {token}");
    let headers = [DeliveryHeader {
        name: "authorization",
        value: authorization.as_bytes(),
    }];
    let check = ingress()
        .pre_auth(
            UntrustedDelivery {
                route,
                received_at_unix_millis: now_millis(now),
                headers: &headers,
                body: br#"{"merge_request_iid":42}"#,
            },
            &*TestClock::at(now_millis(now)),
        )
        .unwrap();
    source.authenticate(check)
}

pub fn accept(
    source: &GitLabOidc,
    claims: &Value,
    body: &[u8],
    now: u64,
) -> Result<AcceptedDelivery, &'static str> {
    let token = sign(claims);
    let authorization = format!("Bearer {token}");
    let route = route();
    let headers = [DeliveryHeader {
        name: "authorization",
        value: authorization.as_bytes(),
    }];
    let raw = UntrustedDelivery {
        route: &route,
        received_at_unix_millis: now_millis(now),
        headers: &headers,
        body,
    };
    let policy = ingress();
    let check = policy
        .pre_auth(raw, &*TestClock::at(now_millis(now)))
        .map_err(|_error| "pre-auth")?;
    let verified = source
        .authenticate(check)
        .map_err(|_error| "authenticate")?;
    policy
        .post_auth(check, verified)
        .map_err(|_error| "post-auth")
}

pub fn ingress() -> IngressPolicy {
    IngressPolicy::new(
        IngressLimits::new(16 * 1024, 8, 32 * 1024).unwrap(),
        ReplayWindow::new(Duration::from_mins(10), Duration::from_mins(1)).unwrap(),
        Duration::from_secs(2),
    )
    .unwrap()
}

fn verify_token(
    source: &GitLabOidc,
    token: &str,
    body: &[u8],
    now: u64,
    duplicate_header: bool,
) -> Result<VerifiedDelivery, ProviderError> {
    let authorization = format!("Bearer {token}");
    let route = route();
    let mut headers = vec![DeliveryHeader {
        name: "authorization",
        value: authorization.as_bytes(),
    }];
    if duplicate_header {
        headers.push(DeliveryHeader {
            name: "Authorization",
            value: authorization.as_bytes(),
        });
    }
    let check = ingress()
        .pre_auth(
            UntrustedDelivery {
                route: &route,
                received_at_unix_millis: now_millis(now),
                headers: &headers,
                body,
            },
            &*TestClock::at(now_millis(now)),
        )
        .unwrap();
    source.authenticate(check)
}

pub fn route() -> DeliveryRoute {
    DeliveryRoute {
        provider: provider(),
        trust_set: OpaqueId::new("gitlab-oidc".to_owned()).unwrap(),
        signed_time: SignedTimePolicy::Required(Duration::from_mins(5)),
    }
}

fn now_millis(now: u64) -> i64 {
    i64::try_from(now).unwrap().checked_mul(1_000).unwrap()
}
