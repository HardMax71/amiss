use amiss_controller::{
    AuthenticatedDelivery, ChangeLocator, DeliveryId, DeliveryIdentity, GiteaWebhook, IngressCheck,
    IntegrationId, ProviderError, ProviderIdentity, SignedTimePolicy, VerifiedDelivery,
    WebhookProof,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, RepositoryIdentity};

use crate::DedicatedReviewer;
use crate::identity::{branch_ref, canonical_host, canonical_segment, change_id, provider_run};
use crate::repository::RepositoryRecord;
use crate::webhook::{HookIssueAction, PullRequestChanges, PullRequestPayload};

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
        // IngressCheck already bounds the signed body before this decoder runs.
        let payload: PullRequestPayload =
            serde_json::from_slice(input.body).map_err(|_defect| ProviderError::Authentication)?;
        let target_edited = matches!(
            payload.changes.as_ref(),
            Some(PullRequestChanges::Gitea { reference: Some(previous), .. }
                | PullRequestChanges::Forgejo { reference: Some(previous), .. })
                if branch_ref(&previous.from).is_some()
        );
        if !(matches!(
            payload.action,
            HookIssueAction::Opened | HookIssueAction::Reopened | HookIssueAction::Synchronized
        ) || payload.action == HookIssueAction::Edited && target_edited)
        {
            return Err(ProviderError::Authentication);
        }
        let repository = payload
            .repository
            .as_ref()
            .ok_or(ProviderError::Authentication)?;
        let pull = payload
            .pull_request
            .as_ref()
            .ok_or(ProviderError::Authentication)?;
        let base = pull
            .base
            .repo
            .as_ref()
            .ok_or(ProviderError::Authentication)?;
        if repository.id == 0
            || pull.id == 0
            || payload.number == 0
            || pull.number != payload.number
            || u64::try_from(pull.base.repo_id) != Ok(repository.id)
            || repository.id != base.id
            || repository.name != base.name
            || repository.full_name != base.full_name
            || repository.owner.login != base.owner.login
            || pull.head.repo.is_none()
            || !repository
                .full_name
                .eq_ignore_ascii_case(&format!("{}/{}", repository.owner.login, repository.name))
        {
            return Err(ProviderError::Authentication);
        }

        let change = ChangeLocator {
            provider: self.provider.clone(),
            repository: repository_identity(&self.provider, repository)
                .ok_or(ProviderError::Authentication)?,
            change: change_id(repository.id, pull.id, payload.number)
                .ok_or(ProviderError::Authentication)?,
        };
        let integration = IntegrationId::try_from(self.reviewer.id.to_string())
            .map_err(|_error| ProviderError::Authentication)?;
        let candidate = pull
            .head
            .sha
            .as_ref()
            .filter(|oid| oid.object_format() == ObjectFormat::Sha1)
            .ok_or(ProviderError::Authentication)?;
        let candidate_ref = branch_ref(&pull.head.branch).ok_or(ProviderError::Authentication)?;
        let target_ref = branch_ref(&pull.base.branch).ok_or(ProviderError::Authentication)?;
        let provider_run = provider_run(
            &integration,
            &change,
            candidate,
            &candidate_ref,
            &target_ref,
        )
        .ok_or(ProviderError::Authentication)?;
        Ok((
            proof,
            PullRequestFacts {
                delivery: AuthenticatedDelivery {
                    identity: DeliveryIdentity {
                        provider: self.provider.clone(),
                        integration,
                        delivery: DeliveryId::try_from(format!(
                            "body:{}",
                            hb(DELIVERY_DOMAIN, input.body)
                        ))
                        .map_err(|_error| ProviderError::Authentication)?,
                    },
                    change,
                    provider_run,
                },
                target_ref,
            },
        ))
    }
}

struct PullRequestFacts {
    delivery: AuthenticatedDelivery,
    target_ref: BranchRef,
}

fn repository_identity(
    provider: &ProviderIdentity,
    repository: &RepositoryRecord,
) -> Option<RepositoryIdentity> {
    RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        canonical_segment(&repository.owner.login)?,
        canonical_segment(&repository.name)?,
    )
}
