use amiss_controller_fixtures::clock::TestClock;
use amiss_wire::branch_ref;
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::de::Document as _;
use amiss_wire::model::Digest;
use std::sync::Arc;
use std::time::Duration;

use amiss_controller::PullRequestChange;
use amiss_controller::{
    AdapterRegistry, AuthenticatedDelivery, Change, ChangeLocator, ChangeSnapshot, ChangeState,
    CheckPlan, Controller, Delivery, DeliveryIdentity, DeliveryLedger, IngressLimits,
    IngressPolicy, OidPair, PlanRegistry, PlanScope, PolicyControls, ProviderIdentity, ProviderRun,
    ProviderRunAttempt, ProviderRunIdentity, ReplayWindow, RunIdentity, RunRefs, RunnerOutcome,
    check_plan, register_plan,
};
use amiss_controller::{opaque_id, provider_namespace};
use amiss_wire::controls::Profile;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::{FakeAdapter, FakeRunner, MemoryLedger};

pub(crate) fn provider() -> ProviderIdentity {
    ProviderIdentity {
        namespace: provider_namespace!("forgejo"),
        instance: opaque_id!("forge.example.test"),
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
        change: Change::PullRequest(PullRequestChange::new(1, 1, 42).unwrap()),
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
            integration: opaque_id!("installation-7"),
            delivery: Delivery::Provided(opaque_id!("delivery-9")),
        },
        change,
        provider_run: ProviderRunIdentity::new(
            ProviderRun::PullRequest(Digest::from([150; 32])),
            ProviderRunAttempt::FIRST,
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
            candidate: branch_ref!("refs/heads/topic"),
            target: branch_ref!("refs/heads/main"),
            default_branch: BranchRef::try_from(default_branch_ref.to_owned()).unwrap(),
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
    let gate_commit = run.commits.candidate.clone();
    ChangeSnapshot {
        state,
        run,
        gate_commit,
    }
}

pub(crate) fn complete(run: &RunIdentity) -> RunnerOutcome {
    RunnerOutcome::Complete {
        identity: Box::new(run.clone()),
        evaluation: amiss_controller::Evaluation::Pass,
        report: br#"{"schema":"amiss/report"}"#.to_vec(),
        semantic_artifact: None,
    }
}

pub(crate) fn plan() -> CheckPlan {
    let execution = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    check_plan(Profile::Enforce, PolicyControls::default(), execution).unwrap()
}

fn ingress() -> IngressPolicy {
    IngressPolicy::new(
        IngressLimits::new(1_024, 32, 8_192).unwrap(),
        ReplayWindow::new(Duration::from_mins(5), Duration::from_secs(30)).unwrap(),
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
    let scope = PlanScope {
        provider: adapter.authenticated.identity.provider.clone(),
        integration: adapter.authenticated.identity.integration.clone(),
        repository: adapter.authenticated.change.repository.clone(),
    };
    let mut plans = PlanRegistry::new();
    register_plan(&mut plans, scope, Arc::new(plan())).unwrap();
    let mut registry = AdapterRegistry::new();
    registry.register(adapter).unwrap();
    Controller::new_with_clock(
        registry,
        plans,
        ledger,
        FakeRunner::new(outcome),
        ingress(),
        TestClock::new(),
    )
}
