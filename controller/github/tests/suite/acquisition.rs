#![expect(
    clippy::unwrap_used,
    reason = "fixed provider identities and constraints must fail loudly"
)]

use amiss_wire::de::Document as _;
use amiss_wire::{artifact_id, repo_path_text};
use sha2::Digest as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use amiss_controller::PullRequestChange;
use amiss_controller::{
    AcquiredSemanticTemplate, Acquisition as _, AcquisitionTarget, Change, ChangeLocator, Delivery,
    DeliveryIdentity, IntegrationId, MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES,
    MAX_WORKFLOW_ARTIFACT_FILE_BYTES, OidPair, PolicyControls, ProviderError, ProviderIdentity,
    ProviderRun, ProviderRunAttempt, ProviderRunIdentity, RunIdentity, RunRefs, RunRequest,
    SemanticEvidenceExpectation, WorkflowArtifactExpectation, check_binding, check_plan,
};
use amiss_controller::{opaque_id, provider_namespace};
use amiss_controller_github::{
    GitFetchBounds, GitHubAcquireError, GitHubAcquisition, GitHubAcquisitionSource,
    github_fetch_plan,
};
use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use secrecy::SecretString;

const RUN_DOMAIN: &str = "amiss/controller-github-pull-request-v2";
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
    wrong_host.run.change.repository = RepositoryIdentity::new(
        "github.com@attacker.invalid".to_owned(),
        "acme".to_owned(),
        "widget".to_owned(),
    )
    .unwrap();
    assert_eq!(
        github_fetch_plan(&wrong_host),
        Err(GitHubAcquireError::InvalidRequest)
    );

    let mut wrong_identity = request();
    wrong_identity.run.change.repository =
        RepositoryIdentity::github("other".to_owned(), "widget".to_owned()).unwrap();
    assert_eq!(
        github_fetch_plan(&wrong_identity),
        Err(GitHubAcquireError::InvalidRequest)
    );

    let mut zero_integration = request();
    let zero = opaque_id!("0");
    zero_integration.provider_run = provider_run(
        &zero,
        &zero_integration.run.change,
        &zero_integration.run.commits.candidate,
        &zero_integration.run.refs.candidate,
        &zero_integration.run.refs.target,
    );
    zero_integration.delivery.integration = zero;
    assert_eq!(
        github_fetch_plan(&zero_integration),
        Err(GitHubAcquireError::InvalidRequest),
        "a zero installation is refused on its own, with its run echo intact"
    );

    let mut wrong_change = request();
    wrong_change.run.change.change =
        Change::PullRequest(PullRequestChange::new(101, 4201, 43).unwrap());
    assert_eq!(
        github_fetch_plan(&wrong_change),
        Err(GitHubAcquireError::InvalidRequest)
    );

    let mut wrong_action_host = request();
    let other_host = RepositoryIdentity::new(
        "other.example".to_owned(),
        "hardmax71".to_owned(),
        "amiss".to_owned(),
    )
    .unwrap();
    Arc::make_mut(&mut wrong_action_host.plan)
        .execution
        .action_repository = other_host;
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
fn workflow_artifacts_stay_on_the_authenticated_repository() {
    let mut exact = request();
    let artifact = workflow_artifact(&exact);
    set_workflow_artifacts(&mut exact, vec![artifact.clone()]);
    assert!(github_fetch_plan(&exact).is_ok());

    let mut foreign = artifact;
    foreign.repository =
        RepositoryIdentity::github("other".to_owned(), "widget".to_owned()).unwrap();
    set_workflow_artifacts(&mut exact, vec![foreign]);
    assert_eq!(
        github_fetch_plan(&exact),
        Err(GitHubAcquireError::InvalidRequest)
    );
}

#[test]
fn planned_artifacts_precede_git_and_cancellation_stops_before_network() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let token_calls = Arc::new(AtomicUsize::new(0));
    let artifact_calls = Arc::new(AtomicUsize::new(0));
    let mut request = request();
    let expectation = workflow_artifact(&request);
    set_workflow_artifacts(&mut request, vec![expectation.clone()]);
    let source = CancellingSource {
        cancelled: Arc::clone(&cancelled),
        token_calls: Arc::clone(&token_calls),
        artifact_calls: Arc::clone(&artifact_calls),
        expectation,
    };
    let mut acquisition = GitHubAcquisition::new(source, GitFetchBounds::default());
    let repository = tempfile::tempdir().unwrap();
    let action = tempfile::tempdir().unwrap();
    let error = acquisition
        .acquire(
            &request,
            AcquisitionTarget {
                repository: repository.path(),
                action: action.path(),
                cancelled,
            },
        )
        .unwrap_err();

    assert_eq!(error, GitHubAcquireError::Cancelled);
    assert_eq!(artifact_calls.load(Ordering::Relaxed), 1);
    assert_eq!(token_calls.load(Ordering::Relaxed), 1);
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

struct CancellingSource {
    cancelled: Arc<AtomicBool>,
    token_calls: Arc<AtomicUsize>,
    artifact_calls: Arc<AtomicUsize>,
    expectation: WorkflowArtifactExpectation,
}

impl GitHubAcquisitionSource for CancellingSource {
    fn installation_token(&self, installation_id: u64) -> Result<SecretString, ProviderError> {
        assert_eq!(installation_id, 7);
        self.token_calls.fetch_add(1, Ordering::Relaxed);
        self.cancelled.store(true, Ordering::Release);
        Ok(SecretString::from(TOKEN.to_owned()))
    }

    fn workflow_artifact(
        &self,
        expectation: &WorkflowArtifactExpectation,
        candidate: &Oid,
    ) -> Result<AcquiredSemanticTemplate, ProviderError> {
        assert_eq!(expectation, &self.expectation);
        assert_eq!(candidate, &oid('b'));
        self.artifact_calls.fetch_add(1, Ordering::Relaxed);
        Ok(AcquiredSemanticTemplate {
            acquisition_identity: expectation.semantic.acquisition_identity.clone(),
            bytes: Arc::from(b"semantic template".as_slice()),
        })
    }
}

fn workflow_artifact(request: &RunRequest) -> WorkflowArtifactExpectation {
    WorkflowArtifactExpectation {
        provider: request.delivery.provider.clone(),
        repository: request.run.change.repository.clone(),
        workflow_identity: opaque_id!("docs-evidence.yml"),
        event: opaque_id!("pull_request"),
        artifact_name: "amiss-semantic-evidence".to_owned(),
        payload_file: repo_path_text!("amiss/semantic-template.json"),
        archive_byte_limit: MAX_WORKFLOW_ARTIFACT_ARCHIVE_BYTES,
        file_byte_limit: MAX_WORKFLOW_ARTIFACT_FILE_BYTES,
        semantic: SemanticEvidenceExpectation {
            acquisition_identity: artifact_id!("github-docs-evidence"),
            producer_kind: amiss_wire::semantic::SemanticProducerKind::SiteBuild,
            producer_identity: artifact_id!("docs-site"),
            producer_version: "0.5.1".to_owned(),
            context_digest: amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix("amiss/test-workflow-context")
                    .chain_update([0_u8])
                    .chain_update(b"docs-site")
                    .finalize()
                    .0,
            ),
        },
    }
}

fn set_workflow_artifacts(
    request: &mut RunRequest,
    workflow_artifacts: Vec<WorkflowArtifactExpectation>,
) {
    let plan = check_plan(
        Profile::Enforce,
        PolicyControls {
            workflow_artifacts,
            ..PolicyControls::default()
        },
        execution(),
    )
    .unwrap();
    request.check = check_binding(&plan).unwrap();
    request.plan = Arc::new(plan);
}

fn request() -> RunRequest {
    let provider = ProviderIdentity {
        namespace: provider_namespace!("github"),
        instance: opaque_id!("github.com"),
    };
    let repository = RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap();
    let change = ChangeLocator {
        provider: provider.clone(),
        repository,
        change: Change::PullRequest(PullRequestChange::new(101, 4201, 42).unwrap()),
    };
    let integration = opaque_id!("7");
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
            delivery: Delivery::Provided(opaque_id!("signed-body")),
        },
        provider_run,
        evaluation_id: opaque_id!("evaluation/1"),
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
    let mut descriptor = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    descriptor.action_repository =
        RepositoryIdentity::github("hardmax71".to_owned(), "amiss".to_owned()).unwrap();
    descriptor.action_object_format = ObjectFormat::Sha1;
    descriptor.action_commit_oid = oid('e');
    descriptor.action_tree_oid = oid('f');
    descriptor
}

fn provider_run(
    installation: &IntegrationId,
    change: &ChangeLocator,
    candidate: &Oid,
    candidate_ref: &BranchRef,
    target_ref: &BranchRef,
) -> ProviderRunIdentity {
    let fields = serde_json::to_vec(&(
        installation.as_str(),
        change.repository.host(),
        change.repository.owner(),
        change.repository.name(),
        change.change,
        candidate.as_str(),
        candidate_ref.as_str(),
        target_ref.as_str(),
    ))
    .unwrap();
    ProviderRunIdentity::new(
        ProviderRun::PullRequest(amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(RUN_DOMAIN)
                .chain_update([0_u8])
                .chain_update(&fields)
                .finalize()
                .0,
        )),
        ProviderRunAttempt::FIRST,
        ObjectFormat::Sha1,
        candidate.clone(),
    )
    .unwrap()
}

fn branch(name: &str) -> BranchRef {
    BranchRef::try_from(format!("refs/heads/{name}")).unwrap()
}

fn oid(value: char) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_string().repeat(40)).unwrap()
}
