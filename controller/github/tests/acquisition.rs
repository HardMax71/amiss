#![expect(
    clippy::unwrap_used,
    reason = "fixed provider identities and constraints must fail loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use amiss_controller::{
    Acquisition as _, AcquisitionTarget, ChangeId, ChangeLocator, ControllerEvaluationId,
    DeliveryId, DeliveryIdentity, IntegrationId, OidPair, PolicyControls, ProviderError,
    ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt, ProviderRunId,
    ProviderRunIdentity, RunIdentity, RunRefs, RunRequest, check_binding, check_plan,
};
use amiss_controller_github::{
    GitFetchBounds, GitHubAcquireError, GitHubAcquisition, GitHubTokenSource, github_fetch_plan,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use secrecy::SecretString;

const RUN_DOMAIN: &str = "amiss/controller-github-pull-request-v1";
const TOKEN: &str = "github_pat_never_print_this";

#[test]
fn projects_only_authenticated_commit_oids_and_the_pinned_action_commit() {
    let request = request();
    let plan = github_fetch_plan(&request).unwrap();

    assert_eq!(plan.installation_id, 7);
    assert_eq!(plan.repository_url, "https://github.com/acme/widget.git");
    assert_eq!(plan.repository_oids, [oid('a'), oid('b')]);
    assert_eq!(plan.action_url, "https://github.com/hardmax71/amiss.git");
    assert_eq!(plan.action_oid, oid('e'));
    assert!(!format!("{plan:?}").contains(TOKEN));
}

#[test]
fn rejects_wrong_host_identity_change_and_object_format() {
    let mut wrong_host = request();
    wrong_host.run.change.repository.host = "github.com@attacker.invalid".to_owned();
    assert_eq!(
        github_fetch_plan(&wrong_host),
        Err(GitHubAcquireError::InvalidRequest)
    );

    let mut wrong_identity = request();
    wrong_identity.run.change.repository.owner = "other".to_owned();
    assert_eq!(
        github_fetch_plan(&wrong_identity),
        Err(GitHubAcquireError::InvalidRequest)
    );

    let mut wrong_change = request();
    wrong_change.run.change.change = ChangeId::new("pull/42".to_owned()).unwrap();
    assert_eq!(
        github_fetch_plan(&wrong_change),
        Err(GitHubAcquireError::InvalidRequest)
    );

    let mut wrong_action_host = request();
    Arc::make_mut(&mut wrong_action_host.plan)
        .execution
        .action_repository
        .host = "other.example".to_owned();
    assert_eq!(
        github_fetch_plan(&wrong_action_host),
        Err(GitHubAcquireError::InvalidRequest)
    );

    let mut wrong_format = request();
    wrong_format.run.object_format = ObjectFormat::Sha256;
    assert_eq!(
        github_fetch_plan(&wrong_format),
        Err(GitHubAcquireError::InvalidRequest)
    );
}

#[test]
fn tree_claims_do_not_change_acquisition_or_steal_runtime_classification() {
    let exact = github_fetch_plan(&request()).unwrap();
    let mut wrong_tree = request();
    wrong_tree.run.trees.candidate = oid('f');

    assert_eq!(github_fetch_plan(&wrong_tree).unwrap(), exact);
}

#[test]
fn cancellation_after_token_issue_stops_before_network_without_leaking_it() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let source = CancellingToken {
        cancelled: Arc::clone(&cancelled),
        calls: Arc::clone(&calls),
    };
    let mut acquisition = GitHubAcquisition::new(source, GitFetchBounds::default());
    let repository = tempfile::tempdir().unwrap();
    let action = tempfile::tempdir().unwrap();
    let error = acquisition
        .acquire(
            &request(),
            AcquisitionTarget {
                repository: repository.path(),
                action: action.path(),
                cancelled,
            },
        )
        .unwrap_err();

    assert_eq!(error, GitHubAcquireError::Cancelled);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(!error.to_string().contains(TOKEN));
    assert!(!format!("{error:?}").contains(TOKEN));
    assert!(repository.path().read_dir().unwrap().next().is_none());
    assert!(action.path().read_dir().unwrap().next().is_none());
}

#[test]
fn fetch_bounds_reject_zero_fractional_and_unbounded_values() {
    assert!(GitFetchBounds::new(Duration::from_mins(1)).is_some());
    for invalid in [
        GitFetchBounds::new(Duration::ZERO),
        GitFetchBounds::new(Duration::from_nanos(1)),
        GitFetchBounds::new(Duration::from_secs(121)),
    ] {
        assert!(invalid.is_none());
    }
}

struct CancellingToken {
    cancelled: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}

impl GitHubTokenSource for CancellingToken {
    fn installation_token(&self, installation_id: u64) -> Result<SecretString, ProviderError> {
        assert_eq!(installation_id, 7);
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.cancelled.store(true, Ordering::Release);
        Ok(SecretString::from(TOKEN.to_owned()))
    }
}

fn request() -> RunRequest {
    let provider = ProviderIdentity {
        namespace: ProviderNamespace::new("github".to_owned()).unwrap(),
        instance: ProviderInstance::new("github.com".to_owned()).unwrap(),
    };
    let repository = RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap();
    let change = ChangeLocator {
        provider: provider.clone(),
        repository,
        change: ChangeId::new("repository/101/pull/4201/number/42".to_owned()).unwrap(),
    };
    let integration = IntegrationId::new("7".to_owned()).unwrap();
    let refs = RunRefs {
        forge: ForgeDialect::Github,
        candidate: branch("topic"),
        target: branch("main"),
        default_branch: branch("main"),
    };
    let candidate = oid('b');
    let provider_run = provider_run(
        &integration,
        &change,
        &candidate,
        &refs.candidate,
        &refs.target,
    );
    let plan =
        Arc::new(check_plan(Profile::Enforce, PolicyControls::default(), execution()).unwrap());
    RunRequest {
        delivery: DeliveryIdentity {
            provider,
            integration,
            delivery: DeliveryId::new("signed-body".to_owned()).unwrap(),
        },
        provider_run,
        evaluation_id: ControllerEvaluationId::new("evaluation/1".to_owned()).unwrap(),
        check: check_binding(&plan).unwrap(),
        plan,
        run: RunIdentity::new(
            change,
            refs,
            ObjectFormat::Sha1,
            OidPair {
                base: oid('a'),
                candidate,
            },
            OidPair {
                base: oid('c'),
                candidate: oid('d'),
            },
        )
        .unwrap(),
    }
}

fn execution() -> ExecutionConstraintDescriptor {
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let mut input = ExecutionConstraintInput::from(&template);
    input.action_repository =
        RepositoryIdentity::github("hardmax71".to_owned(), "amiss".to_owned()).unwrap();
    input.action_object_format = ObjectFormat::Sha1;
    input.action_commit_oid = oid('e');
    input.action_tree_oid = oid('f');
    ExecutionConstraintDescriptor::new(input).unwrap()
}

fn provider_run(
    installation: &IntegrationId,
    change: &ChangeLocator,
    candidate: &Oid,
    candidate_ref: &BranchRef,
    target_ref: &BranchRef,
) -> ProviderRunIdentity {
    let fields = serde_json::to_vec(&[
        installation.as_str(),
        change.repository.host.as_str(),
        change.repository.owner.as_str(),
        change.repository.name.as_str(),
        change.change.as_str(),
        candidate.as_str(),
        candidate_ref.as_str(),
        target_ref.as_str(),
    ])
    .unwrap();
    ProviderRunIdentity::new(
        ProviderRunId::new(format!("pr:{}", hb(RUN_DOMAIN, &fields))).unwrap(),
        ProviderRunAttempt::new(1).unwrap(),
        ObjectFormat::Sha1,
        candidate.clone(),
    )
    .unwrap()
}

fn branch(name: &str) -> BranchRef {
    BranchRef::new(format!("refs/heads/{name}")).unwrap()
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}
