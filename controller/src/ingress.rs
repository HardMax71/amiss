mod binding;
mod policy;

use std::fmt;
use std::time::Duration;

use crate::{AuthenticatedDelivery, DeliveryId, OpaqueId, ProviderIdentity};
pub(crate) use binding::RequestBinding;

pub use policy::{IngressCheck, IngressError, IngressLimits, IngressPolicy};

pub type TrustSetId = OpaqueId;
pub type TrustAnchorId = OpaqueId;

/// Controller-owned routing data selected before request bytes are trusted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryRoute {
    pub provider: ProviderIdentity,
    pub trust_set: TrustSetId,
    pub signed_time: SignedTimePolicy,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeliveryHeader<'a> {
    pub name: &'a str,
    pub value: &'a [u8],
}

impl fmt::Debug for DeliveryHeader<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "DeliveryHeader {{ name: {:?}, value: [REDACTED], value_bytes: {} }}",
            self.name,
            self.value.len()
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct UntrustedDelivery<'a> {
    pub route: &'a DeliveryRoute,
    pub received_at_unix_millis: i64,
    pub headers: &'a [DeliveryHeader<'a>],
    pub body: &'a [u8],
}

impl fmt::Debug for UntrustedDelivery<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UntrustedDelivery")
            .field("route", &self.route)
            .field("received_at_unix_millis", &self.received_at_unix_millis)
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// The authenticated value used to suppress delivery replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayIdentity {
    Authenticated(DeliveryId),
    ExactBody,
}

/// Fixed ceilings used to decide when an authenticated delivery can no longer
/// enter the ledger as new work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayWindow {
    max_signed_age_millis: i64,
    max_queue_age_millis: i64,
}

impl ReplayWindow {
    pub fn new(max_signed_age: Duration, max_queue_age: Duration) -> Option<Self> {
        Some(Self {
            max_signed_age_millis: policy::positive_millis(max_signed_age)?,
            max_queue_age_millis: policy::positive_millis(max_queue_age)?,
        })
    }

    pub(crate) const fn max_signed_age_millis(self) -> i64 {
        self.max_signed_age_millis
    }

    pub(crate) const fn max_queue_age_millis(self) -> i64 {
        self.max_queue_age_millis
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplayKeep {
    Permanent,
    KeepThrough {
        unix_millis: i64,
        window: ReplayWindow,
    },
}

/// A normalized delivery whose replay lifetime was chosen by trusted ingress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedDelivery {
    delivery: AuthenticatedDelivery,
    replay_keep: ReplayKeep,
}

impl AcceptedDelivery {
    /// Marks an authenticated replay identity as unsafe to age out locally.
    #[must_use]
    pub const fn permanent(delivery: AuthenticatedDelivery) -> Self {
        Self {
            delivery,
            replay_keep: ReplayKeep::Permanent,
        }
    }

    /// Returns the authenticated provider facts used by adapters and runners.
    pub const fn delivery(&self) -> &AuthenticatedDelivery {
        &self.delivery
    }

    /// Returns the inclusive replay end for bounded deliveries.
    pub const fn replay_keep_through_unix_millis(&self) -> Option<i64> {
        match self.replay_keep {
            ReplayKeep::Permanent => None,
            ReplayKeep::KeepThrough { unix_millis, .. } => Some(unix_millis),
        }
    }

    pub(crate) const fn keep_through(
        delivery: AuthenticatedDelivery,
        unix_millis: i64,
        window: ReplayWindow,
    ) -> Self {
        Self {
            delivery,
            replay_keep: ReplayKeep::KeepThrough {
                unix_millis,
                window,
            },
        }
    }

    pub(crate) const fn replay_keep(&self) -> ReplayKeep {
        self.replay_keep
    }
}

/// Provider facts plus transient proof details from a successful verifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedDelivery {
    delivery: AuthenticatedDelivery,
    trust_set: TrustSetId,
    anchor: TrustAnchorId,
    issued_at_unix_millis: Option<i64>,
    replay: ReplayIdentity,
    request: RequestBinding,
}

impl VerifiedDelivery {
    pub(crate) fn from_webhook(
        delivery: AuthenticatedDelivery,
        trust_set: TrustSetId,
        anchor: TrustAnchorId,
        issued_at_unix_millis: Option<i64>,
        replay: ReplayIdentity,
        request: RequestBinding,
    ) -> Self {
        Self {
            delivery,
            trust_set,
            anchor,
            issued_at_unix_millis,
            replay,
            request,
        }
    }

    pub fn delivery(&self) -> &AuthenticatedDelivery {
        &self.delivery
    }

    pub fn trust_set(&self) -> &TrustSetId {
        &self.trust_set
    }

    pub fn anchor(&self) -> &TrustAnchorId {
        &self.anchor
    }

    pub const fn issued_at_unix_millis(&self) -> Option<i64> {
        self.issued_at_unix_millis
    }

    pub fn replay(&self) -> &ReplayIdentity {
        &self.replay
    }
}

/// The signed request time required by one controller-owned route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignedTimePolicy {
    ReplayOnly,
    Required(Duration),
}
