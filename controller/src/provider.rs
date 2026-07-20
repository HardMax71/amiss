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
pub enum ProviderErrorKind {
    Authentication,
    AuthorizationRevoked,
    Unavailable,
    InvalidResponse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderError {
    kind: ProviderErrorKind,
}

impl ProviderError {
    #[must_use]
    pub const fn new(kind: ProviderErrorKind) -> Self {
        Self { kind }
    }

    #[must_use]
    pub const fn kind(&self) -> ProviderErrorKind {
        self.kind
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            ProviderErrorKind::Authentication => "provider delivery authentication failed",
            ProviderErrorKind::AuthorizationRevoked => "provider authorization was revoked",
            ProviderErrorKind::Unavailable => "provider is unavailable",
            ProviderErrorKind::InvalidResponse => "provider returned an invalid response",
        })
    }
}

impl std::error::Error for ProviderError {}

pub trait ProviderAdapter: Send + Sync {
    fn namespace(&self) -> &ProviderNamespace;

    /// # Errors
    ///
    /// Returns an authentication or provider error without treating any body
    /// field as trusted input before provider authentication succeeds.
    fn authenticate(
        &self,
        delivery: UntrustedDelivery<'_>,
    ) -> Result<AuthenticatedDelivery, ProviderError>;

    /// Resolves the authenticated delivery's exact provider run ID and
    /// attempt, including whether that run has since been superseded. It must
    /// never substitute the change's current head for the event-bound
    /// candidate.
    ///
    /// # Errors
    ///
    /// Returns an error when that exact authoritative run state cannot be
    /// obtained.
    fn refresh(&self, delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError>;

    /// # Errors
    ///
    /// Updates the exact provider check idempotently by the publication's
    /// controller evaluation ID. Returns an error when that update cannot be
    /// confirmed.
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
    #[must_use]
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

    #[must_use]
    pub fn get(&self, namespace: &ProviderNamespace) -> Option<&dyn ProviderAdapter> {
        self.adapters.get(namespace).map(AsRef::as_ref)
    }
}
