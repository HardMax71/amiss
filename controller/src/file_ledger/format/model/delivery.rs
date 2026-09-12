use amiss_wire::model::RepositoryIdentity;
use serde::{Deserialize, Serialize};

use crate::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, IntegrationId,
    ProviderIdentity, ProviderInstance, ProviderNamespace,
};

use super::{MaterializeResult, checked};
use crate::ProviderRunIdentity;

#[derive(Serialize)]
pub(in crate::file_ledger::format) struct StoredDeliveryKey<'a> {
    provider_namespace: &'a ProviderNamespace,
    provider_instance: &'a ProviderInstance,
    integration: &'a IntegrationId,
    delivery: &'a DeliveryId,
}

impl<'a> StoredDeliveryKey<'a> {
    pub(in crate::file_ledger::format) fn new(identity: &'a DeliveryIdentity) -> Self {
        Self {
            provider_namespace: &identity.provider.namespace,
            provider_instance: &identity.provider.instance,
            integration: &identity.integration,
            delivery: &identity.delivery,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredDelivery {
    identity: DeliveryIdentity,
    change: StoredChange,
    provider_run: ProviderRunIdentity,
}

impl StoredDelivery {
    pub(in crate::file_ledger::format) fn new(delivery: &AuthenticatedDelivery) -> Self {
        Self {
            identity: delivery.identity.clone(),
            change: StoredChange::new(&delivery.change),
            provider_run: delivery.provider_run.clone(),
        }
    }

    pub(in crate::file_ledger::format) fn materialize(
        &self,
    ) -> MaterializeResult<AuthenticatedDelivery> {
        let provider_run = checked(ProviderRunIdentity::new(
            self.provider_run.run_id.clone(),
            self.provider_run.attempt,
            self.provider_run.object_format,
            self.provider_run.candidate_commit.clone(),
        ))?;
        Ok(AuthenticatedDelivery {
            identity: self.identity.clone(),
            change: self.change.materialize()?,
            provider_run,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredChange {
    provider: ProviderIdentity,
    repository: StoredRepository,
    change: ChangeId,
}

impl StoredChange {
    pub(in crate::file_ledger::format) fn new(change: &ChangeLocator) -> Self {
        Self {
            provider: change.provider.clone(),
            repository: StoredRepository::new(&change.repository),
            change: change.change.clone(),
        }
    }

    pub(in crate::file_ledger::format) fn materialize(&self) -> MaterializeResult<ChangeLocator> {
        Ok(ChangeLocator {
            provider: self.provider.clone(),
            repository: self.repository.materialize()?,
            change: self.change.clone(),
        })
    }
}

// The stored frame fixes host/owner/name order, unlike the report identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRepository {
    host: String,
    owner: String,
    name: String,
}

impl StoredRepository {
    fn new(repository: &RepositoryIdentity) -> Self {
        Self {
            host: repository.host().to_owned(),
            owner: repository.owner().to_owned(),
            name: repository.name().to_owned(),
        }
    }

    fn materialize(&self) -> MaterializeResult<RepositoryIdentity> {
        checked(RepositoryIdentity::new(
            self.host.clone(),
            self.owner.clone(),
            self.name.clone(),
        ))
    }
}
