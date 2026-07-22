#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid identities"
)]

use std::sync::Arc;

use amiss_controller::{
    AcquireError, AcquiredRoots, ChangeId, ChangeLocator, ControllerEvaluationId, DeliveryId,
    DeliveryIdentity, IntegrationId, OidPair, PolicyControls, ProviderIdentity, ProviderInstance,
    ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, RunIdentity,
    RunRefs, RunRequest, check_binding, check_plan, verify_acquired,
};
use amiss_fixtures::{CommitPair, commit_pair, git};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

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
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let mut input = ExecutionConstraintInput::from(&template);
    input.action_commit_oid = oid(&action.candidate);
    input.action_tree_oid = action_tree;
    ExecutionConstraintDescriptor::new(input).unwrap()
}

fn request(repository_pair: &CommitPair, action: &CommitPair) -> RunRequest {
    let provider = ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("gitlab.example.internal".to_owned()).unwrap(),
    };
    let execution = action_execution(action, tree(action, &action.candidate));
    let plan =
        Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution).unwrap());
    RunRequest {
        delivery: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("project-hook/7".to_owned()).unwrap(),
            delivery: DeliveryId::new("webhook/9".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("pipeline/987654321:job-42".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            oid(&repository_pair.candidate),
        )
        .unwrap(),
        evaluation_id: ControllerEvaluationId::new("evaluation/11".to_owned()).unwrap(),
        check: check_binding(&plan).unwrap(),
        plan,
        run: RunIdentity::new(
            ChangeLocator {
                provider,
                repository: repository(),
                change: ChangeId::new("merge-request/42".to_owned()).unwrap(),
            },
            RunRefs {
                forge: ForgeDialect::Gitlab,
                candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
                target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
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
    request.check.required_status_name = "amiss / another check".to_owned();

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
