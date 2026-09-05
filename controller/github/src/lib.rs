#![forbid(unsafe_code)]

mod acquisition;
mod live;
mod workflow_artifact;

use std::sync::Arc;

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, ChangeSnapshot, ChangeState, CheckConclusion,
    DeliveryId, DeliveryIdentity, GitHubWebhook, IngressCheck, IntegrationId, OpaqueId,
    ProviderAdapter, ProviderError, ProviderIdentity, ProviderNamespace, ProviderRunAttempt,
    ProviderRunId, ProviderRunIdentity, Publication, SignedTimePolicy, VerifiedDelivery,
    WebhookProof, WorkflowArtifactExpectation,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use serde::Deserialize;

pub use acquisition::{
    GitFetchBounds, GitHubAcquireError, GitHubAcquisition, GitHubAcquisitionSource,
    GitHubFetchPlan, github_fetch_plan,
};
pub use live::{GitHubApp, GitHubClientError, GitHubTimeouts};
pub use workflow_artifact::{GitHubArtifactError, decode_workflow_artifact};

const RUN_DOMAIN: &str = "amiss/controller-github-pull-request-v1";
const SUPPORTED_ACTIONS: [&str; 3] = ["opened", "reopened", "synchronize"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitHubPullRequest<'a> {
    pub change: &'a ChangeLocator,
    pub installation_id: u64,
    pub repository_id: u64,
    pub repository_owner: &'a str,
    pub repository_name: &'a str,
    pub pull_request_id: u64,
    pub number: u64,
    pub candidate_commit: &'a Oid,
}

pub trait GitHubApi: Send + Sync {
    /// Fetches the current state of the exact authenticated pull request.
    ///
    /// # Errors
    ///
    /// The provider state cannot be obtained or authenticated.
    fn refresh(&self, pull_request: GitHubPullRequest<'_>)
    -> Result<ChangeSnapshot, ProviderError>;

    /// Publishes one already-staged result under the authenticated source.
    ///
    /// # Errors
    ///
    /// The provider does not confirm the update.
    fn publish(
        &self,
        pull_request: GitHubPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError>;

    /// External verification, `None` from the default for an API
    /// without a verifier.
    ///
    /// # Errors
    ///
    /// No fact could be gathered before the first one.
    fn verify_external(
        &self,
        _plan: &[u8],
        _checked_at: &str,
    ) -> Result<Option<Vec<u8>>, ProviderError> {
        Ok(None)
    }
}

struct WorkflowCompletion {
    repository: RepositoryIdentity,
    workflow_identity: OpaqueId,
    event: OpaqueId,
}

pub struct GitHubPullRequestSource {
    provider: ProviderIdentity,
    webhook: GitHubWebhook,
    workflow_completion: Option<WorkflowCompletion>,
}

impl GitHubPullRequestSource {
    pub fn new(
        provider: ProviderIdentity,
        webhook: GitHubWebhook,
        workflow_artifacts: &[WorkflowArtifactExpectation],
    ) -> Self {
        let workflow_completion = workflow_artifacts
            .first()
            .filter(|first| {
                workflow_artifacts.iter().all(|artifact| {
                    artifact.provider == provider
                        && artifact.repository == first.repository
                        && artifact.workflow_identity == first.workflow_identity
                        && artifact.event == first.event
                })
            })
            .map(|first| WorkflowCompletion {
                repository: first.repository.clone(),
                workflow_identity: first.workflow_identity.clone(),
                event: first.event.clone(),
            });
        Self {
            provider,
            webhook,
            workflow_completion,
        }
    }

    /// Authenticates one signed GitHub delivery projected onto a pull request
    /// without provider network access.
    ///
    /// # Errors
    ///
    /// The route, signature, or signed payload is invalid.
    pub fn authenticate(&self, check: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        let (proof, facts) = self
            .authenticate_facts(check)?
            .ok_or(ProviderError::Authentication)?;
        Ok(proof.bind(facts.delivery))
    }

    /// Authenticates work only when its signed target is this lane's target.
    /// Authenticated deliveries without work return `None`.
    ///
    /// # Errors
    ///
    /// The request is invalid, or its signed target is outside the configured lane.
    pub fn authenticate_for_target(
        &self,
        check: IngressCheck<'_>,
        target: &BranchRef,
    ) -> Result<Option<VerifiedDelivery>, ProviderError> {
        let Some((proof, facts)) = self.authenticate_facts(check)? else {
            return Ok(None);
        };
        if facts.target_ref != *target {
            return Err(ProviderError::AuthorizationRevoked);
        }
        Ok(Some(proof.bind(facts.delivery)))
    }

    fn authenticate_facts(
        &self,
        check: IngressCheck<'_>,
    ) -> Result<Option<(WebhookProof, PullRequestFacts)>, ProviderError> {
        let proof = self
            .webhook
            .verify(check)
            .map_err(|_defect| ProviderError::Authentication)?;
        let input = check.delivery();
        if input.route.provider != self.provider
            || input.route.signed_time != SignedTimePolicy::ReplayOnly
        {
            return Err(ProviderError::Authentication);
        }
        let Some(facts) = PullRequestFacts::decode(
            input.body,
            &self.provider,
            self.workflow_completion.as_ref(),
        )?
        else {
            return Ok(None);
        };
        Ok(Some((proof, facts)))
    }
}

pub struct GitHubPullRequestAdapter<A> {
    source: Arc<GitHubPullRequestSource>,
    api: A,
}

impl<A> GitHubPullRequestAdapter<A> {
    pub fn new(provider: ProviderIdentity, webhook: GitHubWebhook, api: A) -> Self {
        Self::from_source(
            Arc::new(GitHubPullRequestSource::new(provider, webhook, &[])),
            api,
        )
    }

    pub const fn from_source(source: Arc<GitHubPullRequestSource>, api: A) -> Self {
        Self { source, api }
    }
}

impl<A: GitHubApi> ProviderAdapter for GitHubPullRequestAdapter<A> {
    fn namespace(&self) -> &ProviderNamespace {
        &self.source.provider.namespace
    }

    fn authenticate(&self, check: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        self.source.authenticate(check)
    }

    fn refresh(&self, delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        let pull_request = validate_delivery(delivery, &self.source.provider)?;
        let snapshot = self.api.refresh(pull_request)?;
        let event_bound = event_bound_run(delivery, &snapshot.run)?;
        Ok(ChangeSnapshot {
            state: if event_bound {
                snapshot.state
            } else {
                ChangeState::Superseded
            },
            run: snapshot.run,
            gate_commit: snapshot.gate_commit,
        })
    }

    fn publish(
        &self,
        delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        let pull_request = validate_delivery(delivery, &self.source.provider)?;
        if publication.provider_run != delivery.provider_run {
            return Err(ProviderError::InvalidResponse);
        }
        let event_bound = event_bound_run(delivery, &publication.run)?;
        if !event_bound && !matches!(publication.conclusion, CheckConclusion::Superseded) {
            return Err(ProviderError::InvalidResponse);
        }
        self.api.publish(pull_request, publication)
    }

    fn verify_external(
        &self,
        plan: &[u8],
        checked_at: &str,
    ) -> Result<Option<Vec<u8>>, ProviderError> {
        self.api.verify_external(plan, checked_at)
    }
}

struct PullRequestFacts {
    delivery: AuthenticatedDelivery,
    target_ref: BranchRef,
}

impl PullRequestFacts {
    fn decode(
        body: &[u8],
        provider: &ProviderIdentity,
        workflow_completion: Option<&WorkflowCompletion>,
    ) -> Result<Option<Self>, ProviderError> {
        use ProviderError::Authentication;

        let payload: GitHubPayload =
            serde_json::from_slice(body).map_err(|_defect| Authentication)?;
        if let Some(pull_request) = payload.pull_request.as_ref() {
            if workflow_completion.is_some() {
                return Ok(None);
            }
            if !supported_action(&payload) {
                return Ok(None);
            }
            let (installation_id, repository_id, repository) =
                authenticated_repository(&payload, provider)?;
            let number = payload.number.and_then(positive).ok_or(Authentication)?;
            if pull_request.number != number
                || payload.repository.as_ref() != Some(&pull_request.base.repo)
            {
                return Err(Authentication);
            }
            return bind_pull_request(
                provider,
                installation_id,
                repository_id,
                repository,
                PullRequestBinding {
                    pull_request_id: pull_request.id,
                    number,
                    candidate: &pull_request.head.sha,
                    candidate_branch: &pull_request.head.branch,
                    target_branch: &pull_request.base.branch,
                },
            )
            .map(Some);
        }

        let Some((completion, run)) = configured_workflow(&payload, workflow_completion) else {
            return Ok(None);
        };
        if run.conclusion.as_deref() != Some("success") {
            return Ok(None);
        }
        let (installation_id, repository_id, repository) =
            authenticated_repository(&payload, provider)?;
        let Some(binding) =
            workflow_pull_request(&payload, run, completion, provider, &repository)?
        else {
            return Ok(None);
        };
        bind_pull_request(
            provider,
            installation_id,
            repository_id,
            repository,
            binding,
        )
        .map(Some)
    }
}

#[derive(Clone, Copy)]
struct PullRequestBinding<'a> {
    pull_request_id: u64,
    number: u64,
    candidate: &'a str,
    candidate_branch: &'a str,
    target_branch: &'a str,
}

fn authenticated_repository(
    payload: &GitHubPayload,
    provider: &ProviderIdentity,
) -> Result<(u64, u64, RepositoryIdentity), ProviderError> {
    let installation_id = payload
        .installation
        .as_ref()
        .and_then(|installation| positive(installation.id))
        .ok_or(ProviderError::Authentication)?;
    let repository = payload
        .repository
        .as_ref()
        .ok_or(ProviderError::Authentication)?;
    let repository_id = positive(repository.id).ok_or(ProviderError::Authentication)?;
    let identity =
        github_repository_identity(provider, repository).ok_or(ProviderError::Authentication)?;
    Ok((installation_id, repository_id, identity))
}

fn github_repository_identity(
    provider: &ProviderIdentity,
    repository: &Repository,
) -> Option<RepositoryIdentity> {
    (repository.full_name == format!("{}/{}", repository.owner.login, repository.name))
        .then_some(())?;
    RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        repository.owner.login.to_ascii_lowercase(),
        repository.name.to_ascii_lowercase(),
    )
}

fn bind_pull_request(
    provider: &ProviderIdentity,
    installation_id: u64,
    repository_id: u64,
    repository: RepositoryIdentity,
    binding: PullRequestBinding<'_>,
) -> Result<PullRequestFacts, ProviderError> {
    use ProviderError::Authentication;

    let pull_request_id = positive(binding.pull_request_id).ok_or(Authentication)?;
    let number = positive(binding.number).ok_or(Authentication)?;
    let change = ChangeLocator {
        provider: provider.clone(),
        repository,
        change: change_id(repository_id, pull_request_id, number).ok_or(Authentication)?,
    };
    let integration = IntegrationId::new(installation_id.to_string()).ok_or(Authentication)?;
    let candidate =
        Oid::new(ObjectFormat::Sha1, binding.candidate.to_owned()).ok_or(Authentication)?;
    let candidate_ref = github_ref(binding.candidate_branch).ok_or(Authentication)?;
    let target_ref = github_ref(binding.target_branch).ok_or(Authentication)?;
    let provider_run = provider_run(
        &integration,
        &change,
        &candidate,
        &candidate_ref,
        &target_ref,
    )
    .ok_or(Authentication)?;
    Ok(PullRequestFacts {
        delivery: AuthenticatedDelivery {
            identity: DeliveryIdentity {
                provider: provider.clone(),
                integration,
                delivery: DeliveryId::new("signed-body".to_owned()).ok_or(Authentication)?,
            },
            change,
            provider_run,
        },
        target_ref,
    })
}

fn configured_workflow<'a>(
    payload: &'a GitHubPayload,
    completion: Option<&'a WorkflowCompletion>,
) -> Option<(&'a WorkflowCompletion, &'a WorkflowRun)> {
    let run = payload.workflow_run.as_ref()?;
    (payload.action.as_deref() == Some("completed")).then_some(())?;
    let completion = completion?;
    (run.event == completion.event.as_str()).then_some(())?;
    let identity = completion.workflow_identity.as_str();
    let matches = identity.parse::<u64>().ok().and_then(positive).map_or_else(
        || {
            payload
                .workflow
                .as_ref()
                .is_some_and(|workflow| workflow.path == format!(".github/workflows/{identity}"))
        },
        |workflow_id| run.workflow_id == workflow_id,
    );
    matches.then_some((completion, run))
}

fn workflow_pull_request<'a>(
    payload: &'a GitHubPayload,
    run: &'a WorkflowRun,
    completion: &WorkflowCompletion,
    provider: &ProviderIdentity,
    repository: &RepositoryIdentity,
) -> Result<Option<PullRequestBinding<'a>>, ProviderError> {
    use ProviderError::Authentication;

    let raw_repository = payload.repository.as_ref().ok_or(Authentication)?;
    let workflow_matches = payload
        .workflow
        .as_ref()
        .is_none_or(|workflow| positive(workflow.id) == Some(run.workflow_id));
    (run.status == "completed"
        && positive(run.id).is_some()
        && positive(run.workflow_id).is_some()
        && positive(run.run_attempt).is_some()
        && workflow_matches
        && run.repository == *raw_repository
        && github_repository_identity(provider, &run.repository).as_ref() == Some(repository)
        && github_repository_identity(provider, &run.head_repository).is_some()
        && completion.repository == *repository)
        .then_some(())
        .ok_or(Authentication)?;
    let Ok(pull_requests) = <&[WorkflowPullRequest; 1]>::try_from(run.pull_requests.as_slice())
    else {
        return Ok(None);
    };
    let [pull_request] = pull_requests;
    (positive(run.head_repository.id).is_some()
        && positive(pull_request.id).is_some()
        && positive(pull_request.number).is_some()
        && positive(pull_request.head.repo.id).is_some()
        && positive(pull_request.base.repo.id).is_some()
        && pull_request.head.sha == run.head_sha
        && pull_request.head.repo.id == run.head_repository.id
        && pull_request.head.repo.name == run.head_repository.name
        && pull_request.base.repo.id == raw_repository.id
        && pull_request.base.repo.name == raw_repository.name
        && Oid::new(ObjectFormat::Sha1, pull_request.base.sha.clone()).is_some())
    .then_some(())
    .ok_or(Authentication)?;
    Ok(Some(PullRequestBinding {
        pull_request_id: pull_request.id,
        number: pull_request.number,
        candidate: &pull_request.head.sha,
        candidate_branch: &pull_request.head.branch,
        target_branch: &pull_request.base.branch,
    }))
}

fn supported_action(payload: &GitHubPayload) -> bool {
    payload.action.as_deref().is_some_and(|action| {
        SUPPORTED_ACTIONS.contains(&action)
            || action == "edited"
                && payload
                    .changes
                    .as_ref()
                    .and_then(|changes| changes.base.as_ref())
                    .is_some_and(|base| github_ref(&base.reference.from).is_some())
    })
}

fn validate_delivery<'a>(
    delivery: &'a AuthenticatedDelivery,
    provider: &ProviderIdentity,
) -> Result<GitHubPullRequest<'a>, ProviderError> {
    let repository = &delivery.change.repository;
    let installation_id = delivery
        .identity
        .integration
        .as_str()
        .parse::<u64>()
        .ok()
        .and_then(positive);
    let change = parse_change_id(delivery.change.change.as_str());
    let run_digest = delivery
        .provider_run
        .run_id
        .as_str()
        .strip_prefix("pr:")
        .and_then(Digest::from_wire);
    if delivery.identity.provider != *provider
        || delivery.change.provider != *provider
        || repository.host() != provider.instance.as_str()
        || RepositoryIdentity::new(
            repository.host().to_owned(),
            repository.owner().to_owned(),
            repository.name().to_owned(),
        )
        .as_ref()
            != Some(repository)
        || delivery.provider_run.attempt.get() != 1
        || delivery.provider_run.object_format != ObjectFormat::Sha1
        || Oid::new(
            ObjectFormat::Sha1,
            delivery.provider_run.candidate_commit.as_str().to_owned(),
        )
        .as_ref()
            != Some(&delivery.provider_run.candidate_commit)
        || run_digest.is_none()
    {
        return Err(ProviderError::InvalidResponse);
    }
    let installation_id = installation_id.ok_or(ProviderError::InvalidResponse)?;
    let (repository_id, pull_request_id, number) = change.ok_or(ProviderError::InvalidResponse)?;
    Ok(GitHubPullRequest {
        change: &delivery.change,
        installation_id,
        repository_id,
        repository_owner: repository.owner(),
        repository_name: repository.name(),
        pull_request_id,
        number,
        candidate_commit: &delivery.provider_run.candidate_commit,
    })
}

fn event_bound_run(
    delivery: &AuthenticatedDelivery,
    run: &amiss_controller::RunIdentity,
) -> Result<bool, ProviderError> {
    let identity = provider_run(
        &delivery.identity.integration,
        &delivery.change,
        &run.commits.candidate,
        &run.refs.candidate,
        &run.refs.target,
    )
    .ok_or(ProviderError::InvalidResponse)?;
    (run.change == delivery.change
        && run.refs.forge == ForgeDialect::Github
        && run.object_format == ObjectFormat::Sha1
        && run.commits.candidate == delivery.provider_run.candidate_commit)
        .then_some(identity == delivery.provider_run)
        .ok_or(ProviderError::InvalidResponse)
}

fn provider_run(
    installation: &IntegrationId,
    change: &ChangeLocator,
    candidate: &Oid,
    candidate_ref: &BranchRef,
    target_ref: &BranchRef,
) -> Option<ProviderRunIdentity> {
    let fields = serde_json::to_vec(&[
        installation.as_str(),
        change.repository.host(),
        change.repository.owner(),
        change.repository.name(),
        change.change.as_str(),
        candidate.as_str(),
        candidate_ref.as_str(),
        target_ref.as_str(),
    ])
    .ok()?;
    ProviderRunIdentity::new(
        ProviderRunId::new(format!("pr:{}", hb(RUN_DOMAIN, &fields)))?,
        ProviderRunAttempt::new(1)?,
        ObjectFormat::Sha1,
        candidate.clone(),
    )
}

fn positive(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn change_id(repository_id: u64, pull_request_id: u64, number: u64) -> Option<ChangeId> {
    ChangeId::new(format!(
        "repository/{repository_id}/pull/{pull_request_id}/number/{number}"
    ))
}

fn parse_change_id(raw: &str) -> Option<(u64, u64, u64)> {
    let mut fields = raw.split('/');
    (fields.next()? == "repository").then_some(())?;
    let repository_id = fields.next()?.parse().ok().and_then(positive)?;
    (fields.next()? == "pull").then_some(())?;
    let pull_request_id = fields.next()?.parse().ok().and_then(positive)?;
    (fields.next()? == "number").then_some(())?;
    let number = fields.next()?.parse().ok().and_then(positive)?;
    fields
        .next()
        .is_none()
        .then_some((repository_id, pull_request_id, number))
}

fn github_ref(branch: &str) -> Option<BranchRef> {
    BranchRef::new(format!("refs/heads/{branch}"))
}

#[derive(Deserialize)]
struct GitHubPayload {
    action: Option<String>,
    changes: Option<PullRequestChanges>,
    installation: Option<Installation>,
    repository: Option<Repository>,
    number: Option<u64>,
    pull_request: Option<PullRequest>,
    workflow: Option<Workflow>,
    workflow_run: Option<WorkflowRun>,
}

#[derive(Deserialize)]
struct PullRequestChanges {
    base: Option<BaseChange>,
}

#[derive(Deserialize)]
struct BaseChange {
    #[serde(rename = "ref")]
    reference: PreviousReference,
}

#[derive(Deserialize)]
struct PreviousReference {
    from: String,
}

#[derive(Deserialize)]
struct Installation {
    id: u64,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
struct Repository {
    id: u64,
    name: String,
    full_name: String,
    owner: Owner,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
struct Owner {
    login: String,
}

#[derive(Deserialize)]
struct PullRequest {
    id: u64,
    number: u64,
    head: Head,
    base: Base,
}

#[derive(Deserialize)]
struct Head {
    sha: String,
    #[serde(rename = "ref")]
    branch: String,
}

#[derive(Deserialize)]
struct Base {
    #[serde(rename = "ref")]
    branch: String,
    repo: Repository,
}

#[derive(Deserialize)]
struct Workflow {
    id: u64,
    path: String,
}

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
    event: String,
    status: String,
    conclusion: Option<String>,
    workflow_id: u64,
    run_attempt: u64,
    head_sha: String,
    repository: Repository,
    head_repository: Repository,
    pull_requests: Vec<WorkflowPullRequest>,
}

#[derive(Deserialize)]
struct WorkflowPullRequest {
    id: u64,
    number: u64,
    head: WorkflowPullRef,
    base: WorkflowPullRef,
}

#[derive(Deserialize)]
struct WorkflowPullRef {
    sha: String,
    #[serde(rename = "ref")]
    branch: String,
    repo: WorkflowRepository,
}

#[derive(Deserialize)]
struct WorkflowRepository {
    id: u64,
    name: String,
}
