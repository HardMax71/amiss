#![forbid(unsafe_code)]

use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, ChangeSnapshot, DeliveryId, DeliveryIdentity,
    GitHubWebhook, IngressCheck, IntegrationId, ProviderAdapter, ProviderError, ProviderIdentity,
    ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity, Publication,
    SignedTimePolicy, VerifiedDelivery,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use serde::Deserialize;

const RUN_DOMAIN: &str = "amiss/controller-github-pull-request-v1";
const SUPPORTED_ACTIONS: [&str; 3] = ["opened", "reopened", "synchronize"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitHubPullRequest<'a> {
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
}

pub struct GitHubPullRequestAdapter<A> {
    provider: ProviderIdentity,
    webhook: GitHubWebhook,
    api: A,
}

impl<A> GitHubPullRequestAdapter<A> {
    pub const fn new(provider: ProviderIdentity, webhook: GitHubWebhook, api: A) -> Self {
        Self {
            provider,
            webhook,
            api,
        }
    }
}

impl<A: GitHubApi> ProviderAdapter for GitHubPullRequestAdapter<A> {
    fn namespace(&self) -> &ProviderNamespace {
        &self.provider.namespace
    }

    fn authenticate(&self, check: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        let proof = self
            .webhook
            .verify(check)
            .map_err(|_| ProviderError::Authentication)?;
        let input = check.delivery();
        if input.route.provider != self.provider
            || input.route.signed_time != SignedTimePolicy::ReplayOnly
        {
            return Err(ProviderError::Authentication);
        }
        let facts = PullRequestFacts::decode(input.body, &self.provider)
            .ok_or(ProviderError::Authentication)?;
        Ok(proof.bind(facts.delivery))
    }

    fn refresh(&self, delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        let pull_request = validate_delivery(delivery, &self.provider)?;
        let snapshot = self.api.refresh(pull_request)?;
        validate_run(delivery, &snapshot.run)?;
        Ok(snapshot)
    }

    fn publish(
        &self,
        delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        let pull_request = validate_delivery(delivery, &self.provider)?;
        if publication.provider_run != delivery.provider_run {
            return Err(ProviderError::InvalidResponse);
        }
        validate_run(delivery, &publication.run)?;
        self.api.publish(pull_request, publication)
    }
}

struct PullRequestFacts {
    delivery: AuthenticatedDelivery,
}

impl PullRequestFacts {
    fn decode(body: &[u8], provider: &ProviderIdentity) -> Option<Self> {
        let payload: PullRequestPayload = serde_json::from_slice(body).ok()?;
        if !SUPPORTED_ACTIONS.contains(&payload.action.as_str()) {
            return None;
        }
        let installation_id = positive(payload.installation.id)?;
        let repository_id = positive(payload.repository.id)?;
        let pull_request_id = positive(payload.pull_request.id)?;
        let number = positive(payload.number)?;
        if payload.pull_request.number != payload.number
            || payload.repository != payload.pull_request.base.repo
            || payload.repository.full_name
                != format!(
                    "{}/{}",
                    payload.repository.owner.login, payload.repository.name
                )
        {
            return None;
        }

        let repository = RepositoryIdentity::new(
            provider.instance.as_str().to_owned(),
            payload.repository.owner.login.to_ascii_lowercase(),
            payload.repository.name.to_ascii_lowercase(),
        )?;
        let change = ChangeLocator {
            provider: provider.clone(),
            repository,
            change: change_id(repository_id, pull_request_id, number)?,
        };
        let integration = IntegrationId::new(installation_id.to_string())?;
        let candidate = Oid::new(ObjectFormat::Sha1, payload.pull_request.head.sha)?;
        let candidate_ref = github_ref(&payload.pull_request.head.branch)?;
        let target_ref = github_ref(&payload.pull_request.base.branch)?;
        let provider_run = provider_run(
            &integration,
            &change,
            &candidate,
            &candidate_ref,
            &target_ref,
        )?;

        Some(Self {
            delivery: AuthenticatedDelivery {
                identity: DeliveryIdentity {
                    provider: provider.clone(),
                    integration,
                    delivery: DeliveryId::new("signed-body".to_owned())?,
                },
                change,
                provider_run,
            },
        })
    }
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
        || repository.host != provider.instance.as_str()
        || RepositoryIdentity::new(
            repository.host.clone(),
            repository.owner.clone(),
            repository.name.clone(),
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
        installation_id,
        repository_id,
        repository_owner: &repository.owner,
        repository_name: &repository.name,
        pull_request_id,
        number,
        candidate_commit: &delivery.provider_run.candidate_commit,
    })
}

fn validate_run(
    delivery: &AuthenticatedDelivery,
    run: &amiss_controller::RunIdentity,
) -> Result<(), ProviderError> {
    let identity = provider_run(
        &delivery.identity.integration,
        &delivery.change,
        &run.commits.candidate,
        &run.refs.candidate,
        &run.refs.target,
    )
    .ok_or(ProviderError::InvalidResponse)?;
    if run.change != delivery.change
        || run.refs.forge != ForgeDialect::Github
        || run.object_format != ObjectFormat::Sha1
        || identity != delivery.provider_run
    {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(())
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
        change.repository.host.as_str(),
        change.repository.owner.as_str(),
        change.repository.name.as_str(),
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
struct PullRequestPayload {
    action: String,
    installation: Installation,
    repository: Repository,
    number: u64,
    pull_request: PullRequest,
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
