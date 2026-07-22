use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use amiss_wire::model::RepositoryIdentity;

use crate::{AuthenticatedDelivery, CheckPlan, IntegrationId, ProviderIdentity, check_binding};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlanScope {
    pub provider: ProviderIdentity,
    pub integration: IntegrationId,
    pub repository: RepositoryIdentity,
}

pub type PlanRegistry = BTreeMap<PlanScope, Arc<CheckPlan>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    Duplicate,
    Missing,
    Invalid,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Duplicate => "the check plan scope is already registered",
            Self::Missing => "no check plan matches the authenticated delivery",
            Self::Invalid => "the check plan changed after validation",
        })
    }
}

impl std::error::Error for PlanError {}

/// Adds one checked plan without replacing live controller configuration.
///
/// # Errors
///
/// The plan is invalid or its exact scope is already present.
pub fn register_plan(
    registry: &mut PlanRegistry,
    scope: PlanScope,
    plan: Arc<CheckPlan>,
) -> Result<(), PlanError> {
    check_binding(&plan).map_err(|_defect| PlanError::Invalid)?;
    match registry.entry(scope) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(plan);
            Ok(())
        }
        std::collections::btree_map::Entry::Occupied(_) => Err(PlanError::Duplicate),
    }
}

/// Selects configuration only from authenticated provider facts.
///
/// # Errors
///
/// The exact provider, integration, and repository scope is missing or the
/// stored plan no longer reproduces its digest.
pub fn resolve_plan(
    registry: &PlanRegistry,
    delivery: &AuthenticatedDelivery,
) -> Result<Arc<CheckPlan>, PlanError> {
    let scope = PlanScope {
        provider: delivery.identity.provider.clone(),
        integration: delivery.identity.integration.clone(),
        repository: delivery.change.repository.clone(),
    };
    registry
        .get(&scope)
        .ok_or(PlanError::Missing)
        .and_then(|plan| {
            check_binding(plan)
                .map(|_binding| Arc::clone(plan))
                .map_err(|_defect| PlanError::Invalid)
        })
}
