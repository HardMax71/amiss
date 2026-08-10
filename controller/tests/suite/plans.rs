#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid controller identities"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, CheckPlan, DeliveryId, DeliveryIdentity,
    IntegrationId, PlanError, PlanRegistry, PlanScope, PolicyControls, ProviderIdentity,
    ProviderInstance, ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity,
    check_binding, check_plan, register_plan, resolve_plan,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

fn plan() -> CheckPlan {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../spec/examples/scanner-execution-constraint.json"),
    )
    .unwrap();
    check_plan(
        Profile::Enforce,
        PolicyControls::default(),
        ExecutionConstraintDescriptor::parse(&bytes).unwrap(),
    )
    .unwrap()
}

fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("gitlab.example.internal".to_owned()).unwrap(),
    }
}

fn repository() -> RepositoryIdentity {
    RepositoryIdentity::new(
        "gitlab.example.internal".to_owned(),
        "platform/security".to_owned(),
        "docs".to_owned(),
    )
    .unwrap()
}

fn integration() -> IntegrationId {
    IntegrationId::new("project-hook/7".to_owned()).unwrap()
}

fn delivery() -> AuthenticatedDelivery {
    let provider = provider();
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: integration(),
            delivery: DeliveryId::new("webhook/9".to_owned()).unwrap(),
        },
        change: ChangeLocator {
            provider,
            repository: repository(),
            change: ChangeId::new("merge-request/42".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("pipeline/11".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap(),
        )
        .unwrap(),
    }
}

fn scope() -> PlanScope {
    PlanScope {
        provider: provider(),
        integration: integration(),
        repository: repository(),
    }
}

#[test]
fn plans_resolve_only_from_the_complete_authenticated_scope() {
    let mut registry: PlanRegistry = BTreeMap::new();
    let selected = Arc::new(plan());
    register_plan(&mut registry, scope(), Arc::clone(&selected)).unwrap();
    let resolved = resolve_plan(&registry, &delivery()).unwrap();
    assert_eq!(resolved.plan, selected);
    assert_eq!(resolved.check, check_binding(&selected).unwrap());
    assert_eq!(
        register_plan(&mut registry, scope(), Arc::new(plan())).unwrap_err(),
        PlanError::Duplicate
    );

    let mut other = delivery();
    other.identity.integration = IntegrationId::new("project-hook/8".to_owned()).unwrap();
    assert_eq!(
        resolve_plan(&registry, &other).unwrap_err(),
        PlanError::Missing
    );
}

#[test]
fn a_mutated_plan_never_leaves_the_registry() {
    let mut changed = plan();
    changed.profile = Profile::Observe;
    let mut registry: PlanRegistry = BTreeMap::new();
    assert_eq!(
        register_plan(&mut registry, scope(), Arc::new(changed)).unwrap_err(),
        PlanError::Invalid
    );
    assert!(registry.is_empty());
}
