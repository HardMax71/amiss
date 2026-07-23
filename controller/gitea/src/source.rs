use amiss_controller::{
    AuthenticatedDelivery, ChangeLocator, DeliveryId, DeliveryIdentity, GiteaWebhook, IngressCheck,
    IntegrationId, ProviderError, ProviderIdentity, SignedTimePolicy, VerifiedDelivery,
    WebhookProof,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use serde::Deserialize;

use crate::DedicatedReviewer;
use crate::identity::{
    branch_ref, canonical_host, canonical_segment, change_id, positive, provider_run,
};

const SUPPORTED_ACTIONS: [&str; 3] = ["opened", "reopened", "synchronized"];
const DELIVERY_DOMAIN: &str = "amiss/controller-gitea-family-delivery-v1";

pub struct GiteaPullRequestSource {
    pub(crate) provider: ProviderIdentity,
    pub(crate) reviewer: DedicatedReviewer,
    webhook: GiteaWebhook,
}

impl GiteaPullRequestSource {
    pub fn new(
        provider: ProviderIdentity,
        reviewer: DedicatedReviewer,
        webhook: GiteaWebhook,
    ) -> Option<Self> {
        (canonical_host(provider.instance.as_str())
            && DedicatedReviewer::new(reviewer.id, reviewer.login.clone()).as_ref()
                == Some(&reviewer))
        .then_some(Self {
            provider,
            reviewer,
            webhook,
        })
    }

    /// Authenticates one signed Gitea-family pull-request delivery without
    /// provider network access.
    ///
    /// # Errors
    ///
    /// The route, signature, or signed pull-request payload is invalid.
    pub fn authenticate(&self, check: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        let (proof, facts) = self.authenticate_facts(check)?;
        Ok(proof.bind(facts.delivery))
    }

    /// Authenticates one delivery only when its signed target is this lane's target.
    ///
    /// # Errors
    ///
    /// The request is invalid, or its signed target is outside the configured lane.
    pub fn authenticate_for_target(
        &self,
        check: IngressCheck<'_>,
        target: &BranchRef,
    ) -> Result<VerifiedDelivery, ProviderError> {
        let (proof, facts) = self.authenticate_facts(check)?;
        if facts.target_ref != *target {
            return Err(ProviderError::AuthorizationRevoked);
        }
        Ok(proof.bind(facts.delivery))
    }

    fn authenticate_facts(
        &self,
        check: IngressCheck<'_>,
    ) -> Result<(WebhookProof, PullRequestFacts), ProviderError> {
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
        let facts = PullRequestFacts::decode(input.body, &self.provider, &self.reviewer)
            .ok_or(ProviderError::Authentication)?;
        Ok((proof, facts))
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
        reviewer: &DedicatedReviewer,
    ) -> Option<Self> {
        let payload: PullRequestPayload = serde_json::from_slice(body).ok()?;
        if !supported_action(&payload) {
            return None;
        }
        let repository_id = positive(payload.repository.id)?;
        let pull_request_id = positive(payload.pull_request.id)?;
        let number = positive(payload.number)?;
        if payload.pull_request.number != payload.number
            || payload.pull_request.base.repo_id != payload.repository.id
            || payload.repository != payload.pull_request.base.repo
            || !payload.repository.full_name.eq_ignore_ascii_case(&format!(
                "{}/{}",
                payload.repository.owner.login, payload.repository.name
            ))
        {
            return None;
        }

        let repository = repository_identity(provider, &payload.repository)?;
        let change = ChangeLocator {
            provider: provider.clone(),
            repository,
            change: change_id(repository_id, pull_request_id, number)?,
        };
        let integration = IntegrationId::new(reviewer.id.to_string())?;
        let candidate = Oid::new(ObjectFormat::Sha1, payload.pull_request.head.sha)?;
        let candidate_ref = branch_ref(&payload.pull_request.head.branch)?;
        let target_ref = branch_ref(&payload.pull_request.base.branch)?;
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
                    delivery: DeliveryId::new(format!("body:{}", hb(DELIVERY_DOMAIN, body)))?,
                },
                change,
                provider_run,
            },
            target_ref,
        })
    }
}

fn supported_action(payload: &PullRequestPayload) -> bool {
    SUPPORTED_ACTIONS.contains(&payload.action.as_str())
        || payload.action == "edited"
            && payload
                .changes
                .as_ref()
                .and_then(|changes| changes.reference.as_ref())
                .is_some_and(|reference| branch_ref(&reference.from).is_some())
}

fn repository_identity(
    provider: &ProviderIdentity,
    repository: &Repository,
) -> Option<RepositoryIdentity> {
    RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        canonical_segment(&repository.owner.login)?,
        canonical_segment(&repository.name)?,
    )
}

#[derive(Deserialize)]
struct PullRequestPayload {
    action: String,
    changes: Option<PullRequestChanges>,
    repository: Repository,
    number: u64,
    pull_request: PullRequest,
}

#[derive(Deserialize)]
struct PullRequestChanges {
    #[serde(rename = "ref")]
    reference: Option<PreviousReference>,
}

#[derive(Deserialize)]
struct PreviousReference {
    from: String,
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
    head: PullRef,
    base: PullRef,
}

#[derive(Deserialize)]
struct PullRef {
    sha: String,
    #[serde(rename = "ref")]
    branch: String,
    repo_id: u64,
    repo: Repository,
}
