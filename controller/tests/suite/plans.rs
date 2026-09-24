#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid controller identities"
)]

use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::de::Document as _;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use amiss_controller::MergeRequestChange;
use amiss_controller::PipelineJob;
use amiss_controller::{
    AuthenticatedDelivery, Change, ChangeLocator, CheckPlan, Delivery, DeliveryIdentity,
    ExternalPolicy, OpaqueId, PlanError, PlanRegistry, PlanScope, PolicyControls, ProviderIdentity,
    ProviderRun, ProviderRunAttempt, ProviderRunIdentity, check_binding, check_plan, register_plan,
    resolve_plan,
};
use amiss_controller::{opaque_id, provider_namespace};
use amiss_wire::controls::Profile;
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
        namespace: provider_namespace!("gitlab"),
        instance: opaque_id!("gitlab.example.internal"),
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

fn integration() -> OpaqueId {
    opaque_id!("project-hook/7")
}

fn delivery() -> AuthenticatedDelivery {
    let provider = provider();
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: integration(),
            delivery: Delivery::Provided(opaque_id!("webhook/9")),
        },
        change: ChangeLocator {
            provider,
            repository: repository(),
            change: Change::MergeRequest(MergeRequestChange::new(1, 42).unwrap()),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRun::Job(PipelineJob::new(11, 1).unwrap()),
            ProviderRunAttempt::FIRST,
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
    other.identity.integration = opaque_id!("project-hook/8");
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

    let mut changed = plan();
    changed.policy.external_policy = ExternalPolicy::Off;
    assert_eq!(
        register_plan(&mut registry, scope(), Arc::new(changed)).unwrap_err(),
        PlanError::Invalid
    );
    assert!(registry.is_empty());
}
