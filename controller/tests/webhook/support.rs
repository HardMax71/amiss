use std::sync::LazyLock;
use std::time::Duration;

use amiss_controller::{
    ControllerClock, DeliveryHeader, DeliveryRoute, IngressCheck, IngressError, IngressLimits,
    IngressPolicy, ProviderIdentity, ProviderInstance, ProviderNamespace, ReplayWindow,
    SignedTimePolicy, TrustAnchorId, TrustSetId, UntrustedDelivery, WebhookKey, WebhookKeyring,
};

pub(crate) const NOW: i64 = 1_744_578_123_000;

static REPLAY_ROUTE: LazyLock<DeliveryRoute> =
    LazyLock::new(|| route(SignedTimePolicy::ReplayOnly));
static SIGNED_ROUTE: LazyLock<DeliveryRoute> =
    LazyLock::new(|| route(SignedTimePolicy::Required(Duration::from_mins(5))));

struct FixedClock(i64);

impl ControllerClock for FixedClock {
    fn now_unix_millis(&self) -> Option<i64> {
        Some(self.0)
    }
}

pub(crate) fn anchor(value: &str) -> TrustAnchorId {
    TrustAnchorId::new(value.to_owned()).unwrap()
}

pub(crate) fn trust_set() -> TrustSetId {
    TrustSetId::new("primary-webhooks".to_owned()).unwrap()
}

pub(crate) fn key(
    id: &str,
    secret: &[u8],
    active_from_unix_millis: i64,
    active_until_unix_millis: Option<i64>,
) -> WebhookKey {
    WebhookKey::new(
        anchor(id),
        secret.to_vec(),
        active_from_unix_millis,
        active_until_unix_millis,
    )
    .unwrap()
}

pub(crate) fn ring(id: &str, secret: &[u8]) -> WebhookKeyring {
    WebhookKeyring::new(trust_set(), vec![key(id, secret, 0, None)]).unwrap()
}

pub(crate) const fn header<'a>(name: &'a str, value: &'a [u8]) -> DeliveryHeader<'a> {
    DeliveryHeader { name, value }
}

pub(crate) fn replay_check<'a>(
    headers: &'a [DeliveryHeader<'a>],
    body: &'a [u8],
    received_at_unix_millis: i64,
) -> Result<IngressCheck<'a>, IngressError> {
    check(&REPLAY_ROUTE, headers, body, received_at_unix_millis)
}

pub(crate) fn signed_check<'a>(
    headers: &'a [DeliveryHeader<'a>],
    body: &'a [u8],
    received_at_unix_millis: i64,
) -> Result<IngressCheck<'a>, IngressError> {
    check(&SIGNED_ROUTE, headers, body, received_at_unix_millis)
}

fn check<'a>(
    route: &'static DeliveryRoute,
    headers: &'a [DeliveryHeader<'a>],
    body: &'a [u8],
    received_at_unix_millis: i64,
) -> Result<IngressCheck<'a>, IngressError> {
    let limits = IngressLimits::new(2 * 1_024 * 1_024, 512, 2 * 1_024 * 1_024)
        .ok_or(IngressError::Policy)?;
    let replay = ReplayWindow::new(Duration::from_mins(5), Duration::from_mins(1))
        .ok_or(IngressError::Policy)?;
    let policy = IngressPolicy::new(limits, replay, Duration::ZERO).ok_or(IngressError::Policy)?;
    policy.pre_auth(
        UntrustedDelivery {
            route,
            received_at_unix_millis,
            headers,
            body,
        },
        &FixedClock(received_at_unix_millis),
    )
}

fn route(signed_time: SignedTimePolicy) -> DeliveryRoute {
    DeliveryRoute {
        provider: ProviderIdentity {
            namespace: ProviderNamespace::new("test".to_owned()).unwrap(),
            instance: ProviderInstance::new("forge.example.test".to_owned()).unwrap(),
        },
        trust_set: trust_set(),
        signed_time,
    }
}
