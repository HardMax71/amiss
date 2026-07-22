use crate::ingress::RequestBinding;
use crate::{
    AuthenticatedDelivery, IngressCheck, ReplayIdentity, TrustAnchorId, TrustSetId,
    VerifiedDelivery,
};

/// Transient proof returned after a provider signature is verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebhookProof {
    trust_set: TrustSetId,
    anchor: TrustAnchorId,
    replay: ReplayIdentity,
    issued_at_unix_millis: Option<i64>,
    request: RequestBinding,
}

impl WebhookProof {
    pub(super) fn new(
        check: IngressCheck<'_>,
        trust_set: TrustSetId,
        anchor: TrustAnchorId,
        replay: ReplayIdentity,
        issued_at_unix_millis: Option<i64>,
    ) -> Self {
        Self {
            trust_set,
            anchor,
            replay,
            issued_at_unix_millis,
            request: check.request_binding(),
        }
    }

    pub fn trust_set(&self) -> &TrustSetId {
        &self.trust_set
    }

    pub fn anchor(&self) -> &TrustAnchorId {
        &self.anchor
    }

    pub fn replay(&self) -> &ReplayIdentity {
        &self.replay
    }

    pub const fn issued_at_unix_millis(&self) -> Option<i64> {
        self.issued_at_unix_millis
    }

    /// Joins authenticated provider facts to the exact verifier proof without
    /// letting an adapter relabel the key ring's trust set.
    #[must_use]
    pub fn bind(self, delivery: AuthenticatedDelivery) -> VerifiedDelivery {
        VerifiedDelivery::from_webhook(
            delivery,
            self.trust_set,
            self.anchor,
            self.issued_at_unix_millis,
            self.replay,
            self.request,
        )
    }
}
