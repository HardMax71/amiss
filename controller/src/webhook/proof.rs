use crate::ingress::RequestBinding;
use crate::{IngressCheck, OpaqueId, ProviderFacts, ReplayIdentity, VerifiedDelivery};

/// Transient proof returned after a provider-controlled signature is verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedRequestProof {
    trust_set: OpaqueId,
    anchor: OpaqueId,
    replay: ReplayIdentity,
    issued_at_unix_millis: Option<i64>,
    request: RequestBinding,
}

impl SignedRequestProof {
    /// Captures the exact checked request after a provider adapter has verified
    /// a non-webhook signature such as a workload identity token.
    ///
    /// The caller is part of the trusted adapter. It must authenticate every
    /// supplied proof field before constructing this value.
    #[must_use]
    pub fn verified(
        check: IngressCheck<'_>,
        trust_set: OpaqueId,
        anchor: OpaqueId,
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

    pub fn trust_set(&self) -> &OpaqueId {
        &self.trust_set
    }

    pub fn anchor(&self) -> &OpaqueId {
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
    pub fn bind(self, facts: ProviderFacts) -> VerifiedDelivery {
        VerifiedDelivery::from_webhook(
            facts,
            self.trust_set,
            self.anchor,
            self.issued_at_unix_millis,
            self.replay,
            self.request,
        )
    }
}
