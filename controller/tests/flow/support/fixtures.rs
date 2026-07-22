use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    AdapterRegistry, AuthenticatedDelivery, ChangeId, ChangeLocator, ChangeSnapshot, ChangeState,
    Controller, ControllerClock, DeliveryId, DeliveryIdentity, DeliveryLedger, IngressLimits,
    IngressPolicy, IntegrationId, OidPair, ProviderIdentity, ProviderInstance, ProviderNamespace,
    ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RunIdentity, RunRefs, RunnerOutcome,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::{FakeAdapter, FakeRunner, MemoryLedger};

pub(crate) fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: ProviderNamespace::new("forgejo".to_owned()).unwrap(),
        instance: ProviderInstance::new("forge.example.test".to_owned()).unwrap(),
    }
}

pub(crate) fn repository(name: &str) -> RepositoryIdentity {
    RepositoryIdentity::new(
        "forge.example.test".to_owned(),
        "owner".to_owned(),
        name.to_owned(),
    )
    .unwrap()
}

pub(crate) fn locator(
    provider: &ProviderIdentity,
    repository: RepositoryIdentity,
) -> ChangeLocator {
    ChangeLocator {
        provider: provider.clone(),
        repository,
        change: ChangeId::new("42".to_owned()).unwrap(),
    }
}

pub(crate) fn delivery(
    provider: &ProviderIdentity,
    change: ChangeLocator,
    candidate_commit: char,
) -> AuthenticatedDelivery {
    AuthenticatedDelivery {
        identity: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("installation-7".to_owned()).unwrap(),
            delivery: DeliveryId::new("delivery-9".to_owned()).unwrap(),
        },
        change,
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("provider-run-11".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            oid(candidate_commit),
        )
        .unwrap(),
    }
}

pub(crate) fn oid(byte: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, byte.to_string().repeat(40)).unwrap()
}

pub(crate) fn run(
    change: ChangeLocator,
    candidate_commit: char,
    candidate_tree: char,
) -> RunIdentity {
    run_with_resolution(
        change,
        candidate_commit,
        candidate_tree,
        ForgeDialect::Gitea,
        "refs/heads/main",
    )
}

pub(crate) fn run_with_resolution(
    change: ChangeLocator,
    candidate_commit: char,
    candidate_tree: char,
    forge: ForgeDialect,
    default_branch_ref: &str,
) -> RunIdentity {
    RunIdentity::new(
        change,
        RunRefs {
            forge,
            candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
            target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            default_branch: BranchRef::new(default_branch_ref.to_owned()).unwrap(),
        },
        ObjectFormat::Sha1,
        OidPair {
            base: oid('a'),
            candidate: oid(candidate_commit),
        },
        OidPair {
            base: oid('c'),
            candidate: oid(candidate_tree),
        },
    )
    .unwrap()
}

pub(crate) fn snapshot(state: ChangeState, run: RunIdentity) -> ChangeSnapshot {
    ChangeSnapshot { state, run }
}

pub(crate) fn complete(run: &RunIdentity) -> RunnerOutcome {
    RunnerOutcome::Complete {
        identity: Box::new(run.clone()),
        evaluation: amiss_controller::Evaluation::Pass,
        report: br#"{"schema":"amiss/report"}"#.to_vec(),
    }
}

struct FixedClock;

impl ControllerClock for FixedClock {
    fn now_unix_millis(&self) -> Option<i64> {
        Some(1_800_000_000_000)
    }
}

fn ingress() -> IngressPolicy {
    IngressPolicy::new(
        IngressLimits::new(1_024, 32, 8_192).unwrap(),
        Duration::from_secs(30),
        Duration::from_secs(5),
    )
    .unwrap()
}

pub(crate) fn controller(
    adapter: Arc<FakeAdapter>,
    outcome: RunnerOutcome,
) -> Controller<MemoryLedger, FakeRunner> {
    controller_with_ledger(adapter, MemoryLedger::default(), outcome)
}

pub(crate) fn controller_with_ledger<L: DeliveryLedger>(
    adapter: Arc<FakeAdapter>,
    ledger: L,
    outcome: RunnerOutcome,
) -> Controller<L, FakeRunner> {
    let mut registry = AdapterRegistry::new();
    registry.register(adapter).unwrap();
    Controller::new_with_clock(
        registry,
        ledger,
        FakeRunner::new(outcome),
        ingress(),
        Arc::new(FixedClock),
    )
}
