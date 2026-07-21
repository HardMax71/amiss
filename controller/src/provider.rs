use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::{
    ChangeLocator, ChangeSnapshot, DeliveryIdentity, ProviderIdentity, ProviderNamespace,
    ProviderRunIdentity, Publication,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeliveryHeader<'a> {
    pub name: &'a str,
    pub value: &'a [u8],
}

#[derive(Clone, Copy, Debug)]
pub struct UntrustedDelivery<'a> {
    /// Controller-owned routing identity, never decoded from `body`.
    pub expected_provider: &'a ProviderIdentity,
    /// Controller-owned receipt time, never decoded from `body`.
    pub received_at_unix_seconds: u64,
    pub headers: &'a [DeliveryHeader<'a>],
    pub body: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedDelivery {
    pub identity: DeliveryIdentity,
    pub change: ChangeLocator,
    pub provider_run: ProviderRunIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderError {
    Authentication,
    AuthorizationRevoked,
    Unavailable,
    InvalidResponse,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Authentication => "provider delivery authentication failed",
            Self::AuthorizationRevoked => "provider authorization was revoked",
            Self::Unavailable => "provider is unavailable",
            Self::InvalidResponse => "provider returned an invalid response",
        })
    }
}

impl std::error::Error for ProviderError {}

pub trait ProviderAdapter: Send + Sync {
    fn namespace(&self) -> &ProviderNamespace;

    /// No body field is trusted before authentication succeeds.
    ///
    /// # Errors
    ///
    /// The delivery cannot be authenticated.
    fn authenticate(
        &self,
        delivery: UntrustedDelivery<'_>,
    ) -> Result<AuthenticatedDelivery, ProviderError>;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryError {
    DuplicateNamespace,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("provider namespace is already registered")
    }
}

impl std::error::Error for RegistryError {}

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
