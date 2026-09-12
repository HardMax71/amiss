use amiss_wire::model::RepositoryIdentity;
use serde::{Deserialize, Serialize};

use crate::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, IntegrationId,
    ProviderIdentity, ProviderInstance, ProviderNamespace,
};

use super::{MaterializeResult, checked};
use crate::ProviderRunIdentity;
use crate::file_ledger::FileLedgerError;

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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredDelivery {
    pub(in crate::file_ledger::format) identity: DeliveryIdentity,
    pub(in crate::file_ledger::format) change: StoredChange,
    pub(in crate::file_ledger::format) provider_run: ProviderRunIdentity,
}

impl StoredDelivery {
    pub(in crate::file_ledger::format) fn new(delivery: &AuthenticatedDelivery) -> Self {
        Self {
            identity: delivery.identity.clone(),
            change: StoredChange::new(&delivery.change),
            provider_run: delivery.provider_run.clone(),
        }
    }

    pub(in crate::file_ledger::format) fn matches(&self, delivery: &AuthenticatedDelivery) -> bool {
        self.identity == delivery.identity
            && self.change.matches(&delivery.change)
            && self.provider_run == delivery.provider_run
    }

    pub(in crate::file_ledger::format) fn validate(&self) -> MaterializeResult<()> {
        (self.change.is_valid()
            && self.provider_run.is_valid()
            && self.identity.provider == self.change.provider)
            .then_some(())
            .ok_or(FileLedgerError::Corrupt)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
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

    pub(in crate::file_ledger::format) fn materialize(self) -> MaterializeResult<ChangeLocator> {
        let Self {
            provider,
            repository: StoredRepository { host, owner, name },
            change,
        } = self;
        Ok(ChangeLocator {
            provider,
            repository: checked(RepositoryIdentity::new(host, owner, name))?,
            change,
        })
    }

    fn matches(&self, change: &ChangeLocator) -> bool {
        self.provider == change.provider
            && self.change == change.change
            && self.repository.host == change.repository.host()
            && self.repository.owner == change.repository.owner()
            && self.repository.name == change.repository.name()
    }

    pub(in crate::file_ledger::format) fn is_valid(&self) -> bool {
        RepositoryIdentity::valid_components(
            &self.repository.host,
            &self.repository.owner,
            &self.repository.name,
        )
    }
}

// The stored frame fixes host/owner/name order, unlike the report identity.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}
