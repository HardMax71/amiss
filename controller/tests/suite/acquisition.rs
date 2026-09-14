#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid identities"
)]

use amiss_wire::de::Document as _;
use amiss_wire::{branch_ref, required_status_name};
use std::sync::Arc;

use amiss_controller::MergeRequestChange;
use amiss_controller::PipelineJob;
use amiss_controller::{
    AcquireError, AcquiredRoots, Change, ChangeLocator, Delivery, DeliveryIdentity, OidPair,
    PolicyControls, ProviderIdentity, ProviderRun, ProviderRunAttempt, ProviderRunIdentity,
    RunIdentity, RunRefs, RunRequest, check_binding, check_plan, verify_acquired,
};
use amiss_controller::{opaque_id, provider_namespace};
use amiss_fixtures::{CommitPair, commit_pair, git};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

fn oid(value: &str) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_owned()).unwrap()
}

fn tree(pair: &CommitPair, commit: &str) -> Oid {
    let revision = format!("{commit}^{{tree}}");
    oid(git(pair.root(), &["rev-parse", &revision]).unwrap().trim())
}

fn repository() -> RepositoryIdentity {
    RepositoryIdentity::new(
        "gitlab.example.internal".to_owned(),
        "platform/security".to_owned(),
        "docs".to_owned(),
    )
    .unwrap()
}

fn action_execution(action: &CommitPair, action_tree: Oid) -> ExecutionConstraintDescriptor {
    let mut descriptor = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    descriptor.action_commit_oid = oid(&action.candidate);
    descriptor.action_tree_oid = action_tree;
    descriptor
}

fn request(repository_pair: &CommitPair, action: &CommitPair) -> RunRequest {
    let provider = ProviderIdentity {
        namespace: provider_namespace!("gitlab"),
        instance: opaque_id!("gitlab.example.internal"),
    };
    let execution = action_execution(action, tree(action, &action.candidate));
    let plan =
        Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution).unwrap());
    RunRequest {
        delivery: DeliveryIdentity {
            provider: provider.clone(),
            integration: opaque_id!("project-hook/7"),
            delivery: Delivery::Provided(opaque_id!("webhook/9")),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRun::Job(PipelineJob::new(987_654_321, 42).unwrap()),
            ProviderRunAttempt::FIRST,
            ObjectFormat::Sha1,
            oid(&repository_pair.candidate),
        )
        .unwrap(),
        evaluation_id: opaque_id!("evaluation/11"),
        check: check_binding(&plan).unwrap(),
        plan,
        run: RunIdentity::new(
            ChangeLocator {
                provider,
                repository: repository(),
                change: Change::MergeRequest(MergeRequestChange::new(1, 42).unwrap()),
            },
            RunRefs {
                forge: ForgeDialect::Gitlab,
                candidate: branch_ref!("refs/heads/topic"),
                target: branch_ref!("refs/heads/main"),
                default_branch: branch_ref!("refs/heads/main"),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: oid(&repository_pair.base),
                candidate: oid(&repository_pair.candidate),
            },
            OidPair {
                base: tree(repository_pair, &repository_pair.base),
                candidate: tree(repository_pair, &repository_pair.candidate),
            },
        )
        .unwrap(),
    }
}

fn fixtures() -> (CommitPair, CommitPair) {
    let repository_pair =
        commit_pair(&[("README.md", "base\n")], &[("README.md", "candidate\n")]).unwrap();
    let action = commit_pair(
        &[("bootstrap", "release one\n")],
        &[("bootstrap", "release two\n")],
    )
    .unwrap();
    (repository_pair, action)
}

#[test]
fn accepts_exact_repository_and_action_trees() {
    let (repository_pair, action) = fixtures();
    let request = request(&repository_pair, &action);

    assert_eq!(
        verify_acquired(
            &request,
            AcquiredRoots {
                repository: repository_pair.root(),
                action: action.root(),
            },
        ),
        Ok(())
    );
}

#[test]
fn rejects_a_repository_commit_bound_to_another_tree() {
    let (repository_pair, action) = fixtures();
    let mut request = request(&repository_pair, &action);
    request.run.trees.candidate = tree(&repository_pair, &repository_pair.base);

    assert_eq!(
        verify_acquired(
            &request,
            AcquiredRoots {
                repository: repository_pair.root(),
                action: action.root(),
            },
        ),
        Err(AcquireError::RepositoryTree)
    );
}

#[test]
fn rejects_an_action_commit_bound_to_another_tree() {
    let (repository_pair, action) = fixtures();
    let mut request = request(&repository_pair, &action);
    let execution = action_execution(&action, tree(&action, &action.base));
    request.plan =
        Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution).unwrap());
    request.check = check_binding(&request.plan).unwrap();

    assert_eq!(
        verify_acquired(
            &request,
            AcquiredRoots {
                repository: repository_pair.root(),
                action: action.root(),
            },
        ),
        Err(AcquireError::ActionTree)
    );
}

#[test]
fn rejects_a_plan_that_no_longer_matches_its_delivery_binding() {
    let (repository_pair, action) = fixtures();
    let mut request = request(&repository_pair, &action);
    request.check.required_status_name = required_status_name!("amiss / another check");

    assert_eq!(
        verify_acquired(
            &request,
            AcquiredRoots {
                repository: repository_pair.root(),
                action: action.root(),
            },
        ),
        Err(AcquireError::PlanBinding)
    );
}
