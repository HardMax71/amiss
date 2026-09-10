mod gitea;
mod tests;

use amiss_controller_fixtures::clock::TestClock;
use std::collections::BTreeSet;
use std::sync::LazyLock;
use std::time::Duration;

use amiss_controller::{
    DeliveryHeader, DeliveryRoute, GitHubWebhook, GiteaWebhook, IngressLimits, IngressPolicy,
    OpaqueId, ProviderIdentity, ReplayWindow, SignedTimePolicy, UntrustedDelivery, WebhookKey,
    WebhookKeyring,
};
use amiss_controller_fixtures::{RsaKeys, rsa_keys};
use amiss_controller_gitea::{DedicatedReviewer, GiteaPullRequestSource};
use amiss_controller_github::GitHubPullRequestSource;
use amiss_controller_github::repository::pull::PullRepositoryRecord;
use amiss_controller_github::webhook::pull::request::PullRequestWebhook;
use amiss_controller_github::webhook::{GitHubPayload, Installation};
use amiss_controller_gitlab::claims::{Claims, RequestHint};
use amiss_controller_gitlab::{GitLabOidc, OidcPublicKey, PolicyBinding, RunnerTrust};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid};
use hmac::{Hmac, KeyInit as _, Mac as _};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use sha2::Sha256;

const WEBHOOK_SECRET: &[u8] = b"amiss-controller-fuzz-webhook-secret";
const GITLAB_HOST: &str = "gitlab.example.test";
const GITLAB_AUDIENCE: &str = "amiss-controller";
const GITLAB_KID: &str = "current";
const REPLAY_RETENTION_MILLIS: i64 = 660_000;

#[expect(
    clippy::expect_used,
    reason = "the generated RSA fixture must remain valid"
)]
static RSA_KEYS: LazyLock<RsaKeys> =
    LazyLock::new(|| rsa_keys().expect("the RSA fixture is valid"));

#[expect(
    clippy::expect_used,
    reason = "the fixed provider target must remain valid"
)]
static TARGET_BRANCH: LazyLock<BranchRef> = LazyLock::new(|| {
    BranchRef::new("refs/heads/main".to_owned()).expect("the fixed target is valid")
});

#[expect(
    clippy::expect_used,
    reason = "the generated OIDC fixture and fixed policy must remain valid"
)]
static GITLAB_OIDC: LazyLock<GitLabOidc> = LazyLock::new(|| {
    GitLabOidc::new(
        provider("gitlab", GITLAB_HOST),
        opaque("gitlab-oidc"),
        format!("https://{GITLAB_HOST}"),
        GITLAB_AUDIENCE.to_owned(),
        PolicyBinding {
            integration: opaque("policy/1"),
            project_id: 101,
            project_path: "acme/widget".to_owned(),
            target_branch: "main".to_owned(),
            job_name: "amiss:policy".to_owned(),
            config_url: format!("https://{GITLAB_HOST}/security/policy.yml"),
            config_commit: oid('f'),
            runners: RunnerTrust {
                gitlab_hosted: true,
                self_hosted_ids: BTreeSet::from([77]),
            },
        },
        vec![
            OidcPublicKey::from_rsa_pem(
                GITLAB_KID.to_owned(),
                opaque("gitlab-key/current"),
                &RSA_KEYS.public_pem,
            )
            .expect("the fixed public key is valid"),
        ],
        2,
    )
    .expect("the fixed OIDC policy is valid")
});

#[expect(
    clippy::expect_used,
    reason = "the generated signing fixture must remain valid"
)]
static GITLAB_SIGNING_KEY: LazyLock<EncodingKey> = LazyLock::new(|| {
    EncodingKey::from_rsa_pem(&RSA_KEYS.private_pem).expect("the fixed private key is valid")
});

#[expect(
    clippy::expect_used,
    reason = "the fixed Gitea-family fixtures must remain valid"
)]
/// Exercises the GitHub and Gitea-family webhook boundaries.
///
/// # Panics
///
/// Panics when an unchanged fixture is refused or an authenticated request no
/// longer satisfies the fixed ingress policy.
pub fn provider_webhooks(data: &[u8]) {
    {
        let exercise = prepare_webhook(data);
        let provider = provider("github", "github.example.test");
        let trust_set = opaque("github-webhooks");
        let source = GitHubPullRequestSource::new(
            provider.clone(),
            GitHubWebhook::new(keyring(trust_set.clone())),
            &[],
        );
        let signature = format!("sha256={}", signature(&exercise.body));
        authenticate_webhook(
            &source,
            &DeliveryRoute {
                provider,
                trust_set,
                signed_time: SignedTimePolicy::ReplayOnly,
            },
            DeliveryHeader {
                name: "x-hub-signature-256",
                value: signature.as_bytes(),
            },
            &exercise,
            |source, check| source.authenticate_for_target(check, &TARGET_BRANCH),
        );
    }
    {
        let exercise = gitea::prepare_webhook(data);
        let provider = provider("gitea", "gitea.example.test");
        let trust_set = opaque("gitea-webhooks");
        let source = GiteaPullRequestSource::new(
            provider.clone(),
            DedicatedReviewer::new(44, "amiss-reviewer".to_owned())
                .expect("the fixed reviewer is valid"),
            GiteaWebhook::new(keyring(trust_set.clone())),
        )
        .expect("the fixed Gitea source is valid");
        let signature = signature(&exercise.body);
        let header = if data.get(1).is_some_and(|byte| byte & 1 == 0) {
            "x-forgejo-signature"
        } else {
            "x-gitea-signature"
        };
        authenticate_webhook(
            &source,
            &DeliveryRoute {
                provider,
                trust_set,
                signed_time: SignedTimePolicy::ReplayOnly,
            },
            DeliveryHeader {
                name: header,
                value: signature.as_bytes(),
            },
            &exercise,
            |source, check| {
                source
                    .authenticate_for_target(check, &TARGET_BRANCH)
                    .map(Some)
            },
        );
    }
}

/// Exercises the GitLab OIDC boundary with one bounded input mutation.
///
/// # Panics
///
/// Panics when an unchanged fixture is refused or an authenticated proof no
/// longer satisfies the fixed ingress policy.
#[expect(
    clippy::expect_used,
    reason = "the typed signing fixture and test clock must be valid"
)]
pub fn gitlab_oidc(data: &[u8]) {
    #[derive(serde::Serialize)]
    struct Body<'a> {
        #[serde(flatten)]
        hint: &'a RequestHint,
        #[serde(skip_serializing_if = "Option::is_none")]
        extra: Option<String>,
    }
    let clock = TestClock::new();
    let now = u64::try_from(clock.now().div_euclid(1_000)).expect("the test instant is positive");
    let selector = selection(data, 13);
    let mutation = data.get(1..).unwrap_or_default();
    let mut claims: Claims = serde_json::from_slice(amiss_fixtures::GITLAB_POLICY_CLAIMS)
        .expect("the shared policy-job fixture is complete");
    claims.iss = format!("https://{GITLAB_HOST}");
    claims.job_config.url = format!("https://{GITLAB_HOST}/security/policy.yml");
    claims.iat = now;
    claims.nbf = now.saturating_sub(1);
    claims.exp = now.checked_add(300).expect("the test expiry fits");
    let mut hint = RequestHint {
        merge_request_iid: 42,
    };
    if let Some(change) = selector
        .checked_sub(1)
        .and_then(|index| GITLAB_MUTATIONS.get(index))
    {
        change(&mut claims, &mut hint, mutation);
    }
    let body = serde_json::to_vec(&Body {
        hint: &hint,
        extra: (selector == 12).then(|| text(mutation)),
    })
    .expect("the typed request serializes");
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(GITLAB_KID.to_owned());
    let token = encode(&header, &claims, &GITLAB_SIGNING_KEY)
        .expect("the typed claims sign with the fixed key");
    let authorization = format!("Bearer {token}");
    let provider = provider("gitlab", GITLAB_HOST);
    let route = DeliveryRoute {
        provider: provider.clone(),
        trust_set: opaque("gitlab-oidc"),
        signed_time: SignedTimePolicy::Required(Duration::from_mins(5)),
    };
    let headers = [DeliveryHeader {
        name: "authorization",
        value: authorization.as_bytes(),
    }];
    let policy = ingress();
    let check = policy
        .pre_auth(
            UntrustedDelivery {
                route: &route,
                received_at_unix_millis: clock.now(),
                headers: &headers,
                body: &body,
            },
            &*clock,
        )
        .expect("the generated request is inside ingress bounds");
    let verified = GITLAB_OIDC.authenticate(check);
    assert!(
        selector != 0 || verified.is_ok(),
        "the unchanged signed OIDC request reaches acceptance"
    );
    if let Ok(verified) = verified {
        let accepted = policy.post_auth(check, verified);
        assert!(
            selector != 0 || accepted.is_ok(),
            "the unchanged OIDC proof satisfies ingress"
        );
        if let Ok(accepted) = accepted {
            assert_eq!(accepted.delivery().identity.provider, provider);
            assert_eq!(
                accepted.delivery().identity.delivery.as_str(),
                format!(
                    "oidc/runner/{}/jti/{}",
                    claims.runner_id,
                    hb("amiss/gitlab-oidc-jti-v1", claims.jti.as_bytes())
                ),
                "the authenticated JTI determines the replay identity"
            );
            let keep_through = i64::try_from(claims.iat)
                .ok()
                .and_then(|value| value.checked_mul(1_000))
                .and_then(|value| value.checked_add(REPLAY_RETENTION_MILLIS));
            assert_eq!(
                accepted.replay_keep_through_unix_millis(),
                keep_through,
                "the replay identity remains through both ingress windows"
            );
        }
    }
}

const GITLAB_MUTATIONS: [fn(&mut Claims, &mut RequestHint, &[u8]); 11] = [
    |c, _, bytes| c.jti = text(bytes),
    |c, _, bytes| {
        if bytes.first().is_some_and(|byte| byte & 1 == 0) {
            "self-hosted"
        } else {
            "untrusted"
        }
        .clone_into(&mut c.runner_environment);
    },
    |c, _, bytes| c.runner_id = number(bytes),
    |c, _, bytes| c.job_project_id = number(bytes),
    |c, _, bytes| c.job_project_path = format!("acme/{}", text(bytes)),
    |c, _, bytes| c.pipeline_id = number(bytes),
    |c, _, bytes| c.job_id = number(bytes),
    |c, _, bytes| c.sha = text(bytes),
    |c, _, bytes| c.job_config.url = format!("https://{GITLAB_HOST}/{}", text(bytes)),
    |_, hint, bytes| hint.merge_request_iid = number(bytes),
    |c, _, bytes| c.iat = number(bytes),
];

struct WebhookExercise<'a> {
    body: Vec<u8>,
    data: &'a [u8],
    target_matches: bool,
    family: &'static str,
}

#[expect(
    clippy::expect_used,
    reason = "generated webhook requests and fixed ingress must remain valid"
)]
fn authenticate_webhook<S>(
    source: &S,
    route: &DeliveryRoute,
    header: DeliveryHeader<'_>,
    exercise: &WebhookExercise<'_>,
    authenticate: impl for<'a> Fn(
        &S,
        amiss_controller::IngressCheck<'a>,
    ) -> Result<
        Option<amiss_controller::VerifiedDelivery>,
        amiss_controller::ProviderError,
    >,
) {
    const NOW: i64 = 1_800_000_000_000;
    let headers = [header];
    let policy = ingress();
    let check = policy
        .pre_auth(
            UntrustedDelivery {
                route,
                received_at_unix_millis: NOW,
                headers: &headers,
                body: &exercise.body,
            },
            &*TestClock::at(NOW),
        )
        .expect("the generated request is inside the ingress bounds");
    let accepted = authenticate(source, check).is_ok_and(|verified| {
        verified.is_some_and(|verified| {
            let accepted = policy
                .post_auth(check, verified)
                .expect("a webhook proof from this route satisfies ingress");
            assert_eq!(&accepted.delivery().identity.provider, &route.provider);
            true
        })
    });
    if selection(exercise.data, 10) == 8 {
        assert_eq!(
            accepted, exercise.target_matches,
            "only the configured {} target authenticates",
            exercise.family
        );
    }
    assert!(
        selection(exercise.data, 10) != 0 || accepted,
        "the unchanged signed {} event reaches acceptance",
        exercise.family
    );
}

#[expect(
    clippy::expect_used,
    reason = "generated webhook JSON must remain serializable"
)]
fn prepare_webhook(data: &[u8]) -> WebhookExercise<'_> {
    type Change = fn(&mut GitHubPayload, &[u8]);
    let mut root: PullRepositoryRecord =
        serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_REPOSITORY)
            .expect("the published webhook repository is complete");
    root.id = 11;
    "widget".clone_into(&mut root.name);
    "acme/widget".clone_into(&mut root.full_name);
    "acme".clone_into(&mut root.owner.login);
    let mut pull: PullRequestWebhook = serde_json::from_slice(amiss_fixtures::GITHUB_WEBHOOK_PULL)
        .expect("the published webhook PR is complete");
    pull.request.id = 33;
    pull.request.number = 42;
    pull.request.head.sha = oid('b');
    "topic".clone_into(&mut pull.request.head.branch);
    "main".clone_into(&mut pull.request.base.branch);
    pull.request.base.repo = Some(root.clone());
    let mut payload = GitHubPayload {
        action: Some("opened".to_owned()),
        changes: None,
        installation: Some(Installation {
            id: 22,
            node_id: "installation-twenty-two".to_owned(),
        }),
        repository: Some(root),
        number: Some(42),
        pull_request: Some(pull),
        review: None,
        comment: None,
        workflow: None,
        workflow_run: None,
    };
    let changes: [(usize, Change); 6] = [
        (1, |p, _| p.action = Some("synchronize".to_owned())),
        (2, |p, bytes| p.action = Some(text(bytes))),
        (3, |p, bytes| p.number = Some(number(bytes))),
        (7, |p, bytes| {
            p.pull_request
                .as_mut()
                .expect("the fixture has a PR")
                .request
                .head
                .branch = text(bytes);
        }),
        (8, |p, bytes| {
            p.pull_request
                .as_mut()
                .expect("the fixture has a PR")
                .request
                .base
                .branch = text(bytes);
        }),
        (9, |p, bytes| {
            p.repository
                .as_mut()
                .expect("the fixture has a repository")
                .name = text(bytes);
        }),
    ];
    let selector = selection(data, 10);
    let mutation = data.get(1..).unwrap_or_default();
    if let Some((_, change)) = changes.iter().find(|(index, _)| *index == selector) {
        change(&mut payload, mutation);
    }
    let pull = payload.pull_request.as_ref().expect("the fixture has a PR");
    let target_matches = pull.request.base.branch == "main";
    let mut body = serde_json::to_string(&payload).expect("the typed payload serializes");
    if matches!(selector, 4 | 5) {
        // Out-of-range IDs bypass the safe-integer model only at the negative wire boundary.
        let (key, value) = if selector == 4 {
            ("number", pull.request.number)
        } else {
            ("id", pull.request.id)
        };
        let original = serde_json::to_string(pull).expect("the complete PR serializes");
        let member = format!("\"{key}\":{value},");
        assert_eq!(original.matches(&member).count(), 1);
        let replacement =
            original.replacen(&member, &format!("\"{key}\":{},", number(mutation)), 1);
        assert_eq!(body.matches(&original).count(), 1);
        body = body.replacen(&original, &replacement, 1);
    }
    if selector == 6 {
        // Malformed commit IDs enter only after the typed payload reaches the wire boundary.
        let original =
            serde_json::to_string(&pull.request.head.sha).expect("the head ID serializes");
        let replacement = serde_json::to_string(&text(mutation)).expect("the mutation serializes");
        assert_eq!(body.matches(&original).count(), 1);
        body = body.replacen(&original, &replacement, 1);
    }
    WebhookExercise {
        body: body.into_bytes(),
        data,
        target_matches,
        family: "GitHub",
    }
}

#[expect(
    clippy::expect_used,
    reason = "the fixed ingress policy must remain internally consistent"
)]
fn ingress() -> IngressPolicy {
    IngressPolicy::new(
        IngressLimits::new(65_536, 8, 32_768).expect("the fixed ingress limits are valid"),
        ReplayWindow::new(Duration::from_mins(10), Duration::from_mins(1))
            .expect("the fixed replay limits are valid"),
        Duration::from_secs(2),
    )
    .expect("the fixed ingress policy is valid")
}

#[expect(
    clippy::expect_used,
    reason = "the fixed webhook trust set must remain valid"
)]
fn keyring(trust_set: OpaqueId) -> WebhookKeyring {
    WebhookKeyring::new(
        trust_set,
        vec![
            WebhookKey::new(opaque("current"), WEBHOOK_SECRET.to_vec(), 0, None)
                .expect("the fixed webhook key is valid"),
        ],
    )
    .expect("the fixed webhook keyring is valid")
}

#[expect(clippy::expect_used, reason = "the fixed HMAC key must remain valid")]
fn signature(body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(WEBHOOK_SECRET).expect("the fixed HMAC key is valid");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

#[expect(
    clippy::expect_used,
    reason = "fixed fuzz-provider identities must remain valid"
)]
fn provider(namespace: &str, host: &str) -> ProviderIdentity {
    ProviderIdentity::new(namespace.to_owned(), host.to_owned())
        .expect("the fixed provider identity is valid")
}

#[expect(
    clippy::expect_used,
    reason = "fixed fuzz-fixture identifiers must remain valid"
)]
fn opaque(value: &str) -> OpaqueId {
    OpaqueId::try_from(value.to_owned()).expect("the fixed opaque ID is valid")
}

#[expect(
    clippy::expect_used,
    reason = "fixed fuzz-fixture object IDs must remain valid"
)]
fn oid(digit: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, digit.to_string().repeat(40))
        .expect("the fixed object ID is valid")
}

fn selection(data: &[u8], variants: usize) -> usize {
    data.first().map_or(0, |byte| {
        usize::from(byte & 0x0f)
            .checked_rem(variants)
            .unwrap_or_default()
    })
}

fn number(data: &[u8]) -> u64 {
    let mut bytes = [0_u8; 8];
    for (slot, byte) in bytes.iter_mut().zip(data.iter().copied()) {
        *slot = byte;
    }
    u64::from_le_bytes(bytes)
}

fn text(data: &[u8]) -> String {
    data.iter()
        .take(128)
        .map(|byte| {
            byte.checked_rem(26)
                .and_then(|offset| b'a'.checked_add(offset))
                .map_or('a', char::from)
        })
        .collect()
}
