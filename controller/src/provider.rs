use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{
    ChangeLocator, ChangeSnapshot, DeliveryIdentity, IngressCheck, ProviderNamespace,
    ProviderRunIdentity, Publication, VerifiedDelivery,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedDelivery {
    pub identity: DeliveryIdentity,
    pub change: ChangeLocator,
    pub provider_run: ProviderRunIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("provider delivery authentication failed")]
    Authentication,
    #[error("provider authorization was revoked")]
    AuthorizationRevoked,
    #[error("provider is unavailable")]
    Unavailable,
    #[error("provider returned an invalid response")]
    InvalidResponse,
}

pub trait ProviderAdapter: Send + Sync {
    fn namespace(&self) -> &ProviderNamespace;

    /// The input has passed controller-owned raw and receipt-time ceilings;
    /// no body field is trusted before authentication succeeds.
    ///
    /// # Errors
    ///
    /// The delivery cannot be authenticated.
    fn authenticate(&self, delivery: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError>;

    /// Must never substitute the change's current head for the event-bound
    /// candidate. Implementations must bound this call below the configured
    /// lease window; unlike supervised runner work, refresh has no heartbeat.
    ///
    /// # Errors
    ///
    /// The exact authoritative run state cannot be obtained.
    fn refresh(&self, delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError>;

    /// A staged publication may be delivered more than once. Repeating it is
    /// idempotent by authenticated delivery and controller evaluation ID; a
    /// different publication under that source-bound key must fail closed.
    ///
    /// # Errors
    ///
    /// The update cannot be confirmed.
    fn publish(
        &self,
        delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError>;

    /// Advisory verification of an external plan's introduced destinations:
    /// the facts this provider can state, as an evidence file, or `None`
    /// from the default when it has no verifier. Runs only after the
    /// delivery's verdict is sealed and must never influence one.
    ///
    /// # Errors
    ///
    /// No fact could be gathered before the first one.
    fn verify_external(
        &self,
        _plan: &amiss_wire::json::Value,
        _checked_at: &str,
    ) -> Result<Option<amiss_wire::json::Value>, ProviderError> {
        Ok(None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("provider namespace is already registered")]
    DuplicateNamespace,
}

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<ProviderNamespace, Arc<dyn ProviderAdapter>>,
}

impl AdapterRegistry {
    pub const fn new() -> Self {
        Self {
            adapters: BTreeMap::new(),
        }
    }

    /// # Errors
    ///
    /// Returns [`RegistryError::DuplicateNamespace`] rather than replacing a
    /// trust implementation at runtime.
    pub fn register(&mut self, adapter: Arc<dyn ProviderAdapter>) -> Result<(), RegistryError> {
        let namespace = adapter.namespace().clone();
        if self.adapters.contains_key(&namespace) {
            return Err(RegistryError::DuplicateNamespace);
        }
        self.adapters.insert(namespace, adapter);
        Ok(())
    }

    pub fn get(&self, namespace: &ProviderNamespace) -> Option<&dyn ProviderAdapter> {
        self.adapters.get(namespace).map(AsRef::as_ref)
    }
}
