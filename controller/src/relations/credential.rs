use std::collections::BTreeMap;

use crate::{IntegrationId, OpaqueId, ProviderIdentity, RelationSubject};

use super::RelationRegistry;

pub struct RelationCredentialRoute<A> {
    pub identity: OpaqueId,
    pub authority: A,
}

pub struct RelationCredentialRouter<A> {
    routes: BTreeMap<OpaqueId, (ProviderIdentity, IntegrationId, A)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelationCredentialError {
    #[error("a required relation credential authority is missing")]
    Missing,
    #[error("a relation credential authority is not used by the frozen registry")]
    Unused,
    #[error("a relation subject does not reproduce its credential's frozen provider scope")]
    Rebound,
}

/// Atomically binds one caller-owned authority to every credential required by a frozen registry.
///
/// Duplicate rows are unused rows. No route can be added or replaced after construction.
///
/// # Errors
///
/// A required authority is missing or a row is unused or repeated.
pub fn relation_credential_router<A>(
    registry: &RelationRegistry,
    routes: Vec<RelationCredentialRoute<A>>,
) -> Result<RelationCredentialRouter<A>, RelationCredentialError> {
    let mut bound = BTreeMap::new();
    for route in routes {
        let Some((provider, integration)) = registry.credentials.get(&route.identity) else {
            return Err(RelationCredentialError::Unused);
        };
        bound
            .insert(
                route.identity,
                (provider.clone(), integration.clone(), route.authority),
            )
            .is_none()
            .then_some(())
            .ok_or(RelationCredentialError::Unused)?;
    }
    (bound.len() == registry.credentials.len())
        .then_some(RelationCredentialRouter { routes: bound })
        .ok_or(RelationCredentialError::Missing)
}

/// Selects the authority bound to one complete registered subject.
///
/// # Errors
///
/// The credential is absent or the subject changes its frozen provider or integration scope.
pub fn relation_authority<'a, A>(
    router: &'a RelationCredentialRouter<A>,
    subject: &RelationSubject,
) -> Result<&'a A, RelationCredentialError> {
    let (provider, integration, authority) = router
        .routes
        .get(&subject.credential)
        .ok_or(RelationCredentialError::Missing)?;
    (*provider == subject.scope.provider && *integration == subject.scope.integration)
        .then_some(authority)
        .ok_or(RelationCredentialError::Rebound)
}
