use amiss_wire::model::RepositoryIdentity;
use serde::{Deserialize, Serialize};

use crate::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, IntegrationId,
    ProviderIdentity, ProviderInstance, ProviderNamespace,
};

use super::run::StoredProviderRun;
use super::{MaterializeResult, checked};

#[derive(Serialize)]
pub(in crate::file_ledger::format) struct StoredDeliveryKey<'a> {
    provider_namespace: &'a str,
    provider_instance: &'a str,
    integration: &'a str,
    delivery: &'a str,
}

impl<'a> StoredDeliveryKey<'a> {
    pub(in crate::file_ledger::format) fn new(identity: &'a DeliveryIdentity) -> Self {
        Self {
            provider_namespace: identity.provider.namespace.as_str(),
            provider_instance: identity.provider.instance.as_str(),
            integration: identity.integration.as_str(),
            delivery: identity.delivery.as_str(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredDelivery {
    identity: StoredDeliveryIdentity,
    change: StoredChange,
    provider_run: StoredProviderRun,
}

impl StoredDelivery {
    pub(in crate::file_ledger::format) fn new(delivery: &AuthenticatedDelivery) -> Self {
        Self {
            identity: StoredDeliveryIdentity::new(&delivery.identity),
            change: StoredChange::new(&delivery.change),
            provider_run: StoredProviderRun::new(&delivery.provider_run),
        }
    }

    pub(in crate::file_ledger::format) fn materialize(
        &self,
    ) -> MaterializeResult<AuthenticatedDelivery> {
        Ok(AuthenticatedDelivery {
            identity: self.identity.materialize()?,
            change: self.change.materialize()?,
            provider_run: self.provider_run.materialize()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDeliveryIdentity {
    provider: StoredProvider,
    integration: String,
    delivery: String,
}

impl StoredDeliveryIdentity {
    fn new(identity: &DeliveryIdentity) -> Self {
        Self {
            provider: StoredProvider::new(&identity.provider),
            integration: identity.integration.as_str().to_owned(),
            delivery: identity.delivery.as_str().to_owned(),
        }
    }

    fn materialize(&self) -> MaterializeResult<DeliveryIdentity> {
        Ok(DeliveryIdentity {
            provider: self.provider.materialize()?,
            integration: checked(IntegrationId::new(self.integration.clone()))?,
            delivery: checked(DeliveryId::new(self.delivery.clone()))?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredProvider {
    namespace: String,
    instance: String,
}

impl StoredProvider {
    fn new(provider: &ProviderIdentity) -> Self {
        Self {
            namespace: provider.namespace.as_str().to_owned(),
            instance: provider.instance.as_str().to_owned(),
        }
    }

    fn materialize(&self) -> MaterializeResult<ProviderIdentity> {
        Ok(ProviderIdentity {
            namespace: checked(ProviderNamespace::new(self.namespace.clone()))?,
            instance: checked(ProviderInstance::new(self.instance.clone()))?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::file_ledger::format) struct StoredChange {
    provider: StoredProvider,
    repository: StoredRepository,
    change: String,
}

impl StoredChange {
    pub(in crate::file_ledger::format) fn new(change: &ChangeLocator) -> Self {
        Self {
            provider: StoredProvider::new(&change.provider),
            repository: StoredRepository::new(&change.repository),
            change: change.change.as_str().to_owned(),
        }
    }

    pub(in crate::file_ledger::format) fn materialize(&self) -> MaterializeResult<ChangeLocator> {
        Ok(ChangeLocator {
            provider: self.provider.materialize()?,
            repository: self.repository.materialize()?,
            change: checked(ChangeId::new(self.change.clone()))?,
        })
    }
}

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
            host: repository.host.clone(),
            owner: repository.owner.clone(),
            name: repository.name.clone(),
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
