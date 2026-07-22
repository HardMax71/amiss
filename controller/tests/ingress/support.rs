use std::time::Duration;

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, ControllerClock, DeliveryHeader, DeliveryId,
    DeliveryIdentity, DeliveryRoute, GitHubWebhook, GitLabWebhook, IngressError, IngressLimits,
    IngressPolicy, IntegrationId, OpaqueId, ProviderIdentity, ProviderInstance, ProviderNamespace,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, ReplayWindow, SignedTimePolicy,
    TrustSetId, UntrustedDelivery, VerifiedDelivery, WebhookKey, WebhookKeyring, WebhookProof,
};
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

pub(crate) const BODY: &[u8] = br#"{"event":"change"}"#;
pub(crate) const GITHUB_SECRET: &[u8] = b"It's a Secret to Everybody";
pub(crate) const GITHUB_HEADERS: &[DeliveryHeader<'_>] = &[DeliveryHeader {
    name: "x-hub-signature-256",
    value: b"sha256=ac6a690197321dcf9b6291614f70f95fc93f096f646a1209c6d9de950ba0cb43",
}];
pub(crate) const GITLAB_BODY: &[u8] = b"{\"object_kind\":\"pipeline\",\"status\":\"success\"}";
pub(crate) const GITLAB_NOW: i64 = 1_744_578_123_000;
pub(crate) const GITLAB_SECRET: &[u8] = b"0123456789abcdef0123456789abcdef";
pub(crate) const GITLAB_HEADERS: &[DeliveryHeader<'_>] = &[
    DeliveryHeader {
        name: "webhook-id",
        value: b"f5e5f430-f57b-4e6e-9fac-d9128cd7232f",
    },
    DeliveryHeader {
        name: "webhook-timestamp",
        value: b"1744578123",
    },
    DeliveryHeader {
        name: "webhook-signature",
        value: b"v1,eoSaLtOFqb9PT8wdg5hLQ8m9BxoPEp7HLufb1Anqlzg=",
    },
];

pub(crate) struct FixedClock(pub(crate) Option<i64>);

impl ControllerClock for FixedClock {
    fn now_unix_millis(&self) -> Option<i64> {
        self.0
    }
}

pub(crate) fn opaque(value: &str) -> OpaqueId {
    OpaqueId::new(value.to_owned()).unwrap()
}

pub(crate) fn provider(instance: &str) -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("forge".to_owned()).unwrap(),
        instance: ProviderInstance::new(instance.to_owned()).unwrap(),
    }
}

pub(crate) fn route(signed_time: SignedTimePolicy) -> DeliveryRoute {
    DeliveryRoute {
        provider: provider("forge.example.test"),
        trust_set: opaque("webhooks-main"),
        signed_time,
    }
}

pub(crate) fn raw<'a>(
    route: &'a DeliveryRoute,
    received_at_unix_millis: i64,
    headers: &'a [DeliveryHeader<'a>],
    body: &'a [u8],
) -> UntrustedDelivery<'a> {
    UntrustedDelivery {
        route,
        received_at_unix_millis,
        headers,
        body,
    }
}

pub(crate) fn delivery(provider: &ProviderIdentity) -> AuthenticatedDelivery {
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("installation-7".to_owned()).unwrap(),
            delivery: DeliveryId::new("untrusted-placeholder".to_owned()).unwrap(),
        },
        change: ChangeLocator {
            provider: provider.clone(),
            repository: RepositoryIdentity::new(
                "forge.example.test".to_owned(),
                "owner".to_owned(),
                "amiss".to_owned(),
            )
            .unwrap(),
            change: ChangeId::new("42".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("run-11".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            Oid::new(ObjectFormat::Sha1, "b".repeat(40)).unwrap(),
        )
        .unwrap(),
    }
}

pub(crate) fn github_verified(
    check: amiss_controller::IngressCheck<'_>,
    provider: &ProviderIdentity,
    trust_set: TrustSetId,
) -> VerifiedDelivery {
    github_proof(check, trust_set).bind(delivery(provider))
}

pub(crate) fn github_proof(
    check: amiss_controller::IngressCheck<'_>,
    trust_set: TrustSetId,
) -> WebhookProof {
    let key = WebhookKey::new(opaque("anchor-2"), GITHUB_SECRET.to_vec(), 0, None).unwrap();
    GitHubWebhook::new(WebhookKeyring::new(trust_set, vec![key]).unwrap())
        .verify(check)
        .unwrap()
}

pub(crate) fn gitlab_verified(
    check: amiss_controller::IngressCheck<'_>,
    provider: &ProviderIdentity,
) -> VerifiedDelivery {
    let key = WebhookKey::new(opaque("anchor-2"), GITLAB_SECRET.to_vec(), 0, None).unwrap();
    GitLabWebhook::new(WebhookKeyring::new(opaque("webhooks-main"), vec![key]).unwrap())
        .verify(check)
        .unwrap()
        .bind(delivery(provider))
}

pub(crate) fn policy(
    max_queue_age: Duration,
    future_skew: Duration,
) -> Result<IngressPolicy, IngressError> {
    let limits = IngressLimits::new(1_024, 16, 2_048).ok_or(IngressError::Policy)?;
    let replay =
        ReplayWindow::new(Duration::from_secs(100), max_queue_age).ok_or(IngressError::Policy)?;
    IngressPolicy::new(limits, replay, future_skew).ok_or(IngressError::Policy)
}
