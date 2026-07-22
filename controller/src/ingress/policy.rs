use std::fmt;
use std::time::Duration;

use amiss_wire::digest::hb;

use super::{
    AcceptedDelivery, ReplayIdentity, ReplayWindow, RequestBinding, SignedTimePolicy,
    UntrustedDelivery, VerifiedDelivery,
};
use crate::{ControllerClock, DeliveryId};

const EXACT_BODY_DOMAIN: &str = "amiss/controller-exact-delivery-v1";

/// Raw request ceilings applied before provider authentication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngressLimits {
    body_bytes: usize,
    header_count: usize,
    header_bytes: usize,
}

impl IngressLimits {
    pub const fn new(
        max_body_bytes: usize,
        max_header_count: usize,
        max_header_bytes: usize,
    ) -> Option<Self> {
        if max_body_bytes == 0 || max_header_count == 0 || max_header_bytes == 0 {
            return None;
        }
        Some(Self {
            body_bytes: max_body_bytes,
            header_count: max_header_count,
            header_bytes: max_header_bytes,
        })
    }

    pub const fn max_body_bytes(self) -> usize {
        self.body_bytes
    }

    pub const fn max_header_count(self) -> usize {
        self.header_count
    }

    pub const fn max_header_bytes(self) -> usize {
        self.header_bytes
    }

    fn accepts(self, delivery: &UntrustedDelivery<'_>) -> bool {
        if delivery.body.len() > self.body_bytes || delivery.headers.len() > self.header_count {
            return false;
        }
        delivery
            .headers
            .iter()
            .try_fold(0_usize, |total, header| {
                total
                    .checked_add(header.name.len())?
                    .checked_add(header.value.len())
            })
            .is_some_and(|total| total <= self.header_bytes)
    }
}

/// Provider-neutral admission rules applied around authentication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngressPolicy {
    limits: IngressLimits,
    replay_window: ReplayWindow,
    future_skew_millis: i64,
}

impl IngressPolicy {
    pub fn new(
        limits: IngressLimits,
        replay_window: ReplayWindow,
        future_skew: Duration,
    ) -> Option<Self> {
        Some(Self {
            limits,
            replay_window,
            future_skew_millis: nonnegative_millis(future_skew)?,
        })
    }

    /// Checks raw request ceilings and controller-owned receipt time.
    ///
    /// # Errors
    ///
    /// Returns an error when the request exceeds a ceiling or trusted time
    /// cannot place its receipt inside the configured queue window.
    pub fn pre_auth<'a>(
        &self,
        delivery: UntrustedDelivery<'a>,
        clock: &dyn ControllerClock,
    ) -> Result<IngressCheck<'a>, IngressError> {
        if !self.limits.accepts(&delivery) {
            return Err(IngressError::Limits);
        }
        let signed_max_age_millis = match delivery.route.signed_time {
            SignedTimePolicy::ReplayOnly => None,
            SignedTimePolicy::Required(max_age) => {
                let max_age = positive_millis(max_age).ok_or(IngressError::Policy)?;
                if max_age > self.replay_window.max_signed_age_millis() {
                    return Err(IngressError::Policy);
                }
                Some(max_age)
            }
        };
        let now = clock
            .now_unix_millis()
            .filter(|value| *value >= 0)
            .ok_or(IngressError::Clock)?;
        check_window(
            delivery.received_at_unix_millis,
            now,
            self.replay_window.max_queue_age_millis(),
            self.future_skew_millis,
        )?;
        let request = RequestBinding::new(&delivery).ok_or(IngressError::Limits)?;
        Ok(IngressCheck {
            delivery,
            now,
            signed_max_age_millis,
            request,
        })
    }

    /// Checks authenticated route and signed-time bindings, then creates the
    /// stable delivery identity used by the ledger.
    ///
    /// # Errors
    ///
    /// Returns an error when provider proof does not match the controller
    /// route, required signed time is unavailable or stale, or replay identity
    /// cannot be constructed.
    pub fn post_auth(
        &self,
        check: IngressCheck<'_>,
        verified: VerifiedDelivery,
    ) -> Result<AcceptedDelivery, IngressError> {
        if verified.request != check.request {
            return Err(IngressError::Request);
        }
        if verified.trust_set != check.delivery.route.trust_set
            || verified.delivery.identity.provider != check.delivery.route.provider
            || verified.delivery.change.provider != check.delivery.route.provider
        {
            return Err(IngressError::Route);
        }
        let issued_at = self.check_signed_time(&check, verified.issued_at_unix_millis)?;
        normalize_delivery(verified, check.delivery.body, issued_at, self.replay_window)
    }

    fn check_signed_time(
        &self,
        check: &IngressCheck<'_>,
        issued_at: Option<i64>,
    ) -> Result<Option<i64>, IngressError> {
        let max_age = match (check.signed_max_age_millis, issued_at) {
            (None, None) => return Ok(None),
            (None, Some(_)) => return Err(IngressError::Policy),
            (Some(_), None) => return Err(IngressError::Freshness),
            (Some(max_age), Some(_)) => max_age,
        };
        let issued_at = issued_at
            .filter(|value| *value >= 0)
            .ok_or(IngressError::Freshness)?;
        check_window(issued_at, check.now, i64::MAX, self.future_skew_millis)?;
        check_window(
            issued_at,
            check.delivery.received_at_unix_millis,
            max_age,
            self.future_skew_millis,
        )?;
        Ok(Some(issued_at))
    }
}

/// A raw delivery that passed the pre-authentication gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IngressCheck<'a> {
    delivery: UntrustedDelivery<'a>,
    now: i64,
    signed_max_age_millis: Option<i64>,
    request: RequestBinding,
}

impl<'a> IngressCheck<'a> {
    pub const fn delivery(&self) -> UntrustedDelivery<'a> {
        self.delivery
    }

    pub(crate) const fn request_binding(&self) -> RequestBinding {
        self.request
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngressError {
    Clock,
    Limits,
    Policy,
    Request,
    Route,
    Freshness,
    Replay,
}

impl fmt::Display for IngressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Clock => "controller time cannot be trusted",
            Self::Limits => "provider delivery exceeds an ingress ceiling",
            Self::Policy => "provider ingress policy is invalid",
            Self::Request => "provider proof does not bind this request",
            Self::Route => "authenticated delivery does not match its route",
            Self::Freshness => "provider delivery is outside its freshness window",
            Self::Replay => "provider replay identity is invalid",
        })
    }
}

impl std::error::Error for IngressError {}

fn normalize_delivery(
    verified: VerifiedDelivery,
    body: &[u8],
    issued_at: Option<i64>,
    replay_window: ReplayWindow,
) -> Result<AcceptedDelivery, IngressError> {
    let mut delivery = verified.delivery;
    match verified.replay {
        ReplayIdentity::Authenticated(delivery_id) => {
            delivery.identity.delivery = delivery_id;
            match issued_at {
                Some(issued_at) => {
                    let unix_millis = issued_at
                        .checked_add(replay_window.max_signed_age_millis())
                        .and_then(|value| value.checked_add(replay_window.max_queue_age_millis()))
                        .ok_or(IngressError::Replay)?;
                    Ok(AcceptedDelivery::keep_through(
                        delivery,
                        unix_millis,
                        replay_window,
                    ))
                }
                None => Ok(AcceptedDelivery::permanent(delivery)),
            }
        }
        ReplayIdentity::ExactBody => {
            delivery.identity.delivery = exact_body_id(body)?;
            Ok(AcceptedDelivery::permanent(delivery))
        }
    }
}

fn exact_body_id(body: &[u8]) -> Result<DeliveryId, IngressError> {
    DeliveryId::new(format!("body:{}", hb(EXACT_BODY_DOMAIN, body))).ok_or(IngressError::Replay)
}

fn check_window(
    value: i64,
    reference: i64,
    maximum_age: i64,
    future_skew: i64,
) -> Result<(), IngressError> {
    if value < 0 || reference < 0 {
        return Err(IngressError::Clock);
    }
    let (distance, limit) = if value > reference {
        (value.checked_sub(reference), future_skew)
    } else {
        (reference.checked_sub(value), maximum_age)
    };
    let distance = distance.ok_or(IngressError::Clock)?;
    (distance <= limit)
        .then_some(())
        .ok_or(IngressError::Freshness)
}

pub(super) fn positive_millis(duration: Duration) -> Option<i64> {
    nonnegative_millis(duration).filter(|value| *value > 0)
}

fn nonnegative_millis(duration: Duration) -> Option<i64> {
    i64::try_from(duration.as_millis()).ok()
}
