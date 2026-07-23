use std::sync::Arc;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, CheckConclusion, GiteaWebhook,
    IngressCheck, ProviderAdapter, ProviderError, ProviderIdentity, ProviderNamespace, Publication,
    VerifiedDelivery,
};
use amiss_wire::digest::Digest;
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use crate::identity::{parse_change_id, positive, provider_run};
use crate::{DedicatedReviewer, GiteaApi, GiteaPullRequest, GiteaPullRequestSource};

pub struct GiteaPullRequestAdapter<A> {
    source: Arc<GiteaPullRequestSource>,
    api: A,
}

impl<A> GiteaPullRequestAdapter<A> {
    pub fn new(
        provider: ProviderIdentity,
        reviewer: DedicatedReviewer,
        webhook: GiteaWebhook,
        api: A,
    ) -> Option<Self> {
        Some(Self::from_source(
            Arc::new(GiteaPullRequestSource::new(provider, reviewer, webhook)?),
            api,
        ))
    }

    pub const fn from_source(source: Arc<GiteaPullRequestSource>, api: A) -> Self {
        Self { source, api }
    }
}

impl<A: GiteaApi> ProviderAdapter for GiteaPullRequestAdapter<A> {
    fn namespace(&self) -> &ProviderNamespace {
        &self.source.provider.namespace
    }

    fn authenticate(&self, check: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        self.source.authenticate(check)
    }

    fn refresh(&self, delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        let pull_request =
            validate_delivery(delivery, &self.source.provider, &self.source.reviewer)?;
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
        let pull_request =
            validate_delivery(delivery, &self.source.provider, &self.source.reviewer)?;
        if publication.provider_run != delivery.provider_run {
            return Err(ProviderError::InvalidResponse);
        }
        let event_bound = event_bound_run(delivery, &publication.run)?;
        if !event_bound && !matches!(publication.conclusion, CheckConclusion::Superseded) {
            return Err(ProviderError::InvalidResponse);
        }
        self.api.publish(pull_request, publication)
    }
}

fn validate_delivery<'a>(
    delivery: &'a AuthenticatedDelivery,
    provider: &ProviderIdentity,
    reviewer: &DedicatedReviewer,
) -> Result<GiteaPullRequest<'a>, ProviderError> {
    let repository = &delivery.change.repository;
    let reviewer_id = delivery
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
    let canonical_repository = RepositoryIdentity::new(
        repository.host.clone(),
        repository.owner.clone(),
        repository.name.clone(),
    )
    .as_ref()
        == Some(repository);
    if delivery.identity.provider != *provider
        || delivery.change.provider != *provider
        || repository.host != provider.instance.as_str()
        || repository.owner.contains('/')
        || !canonical_repository
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
    let reviewer_id = reviewer_id
        .filter(|id| *id == reviewer.id)
        .ok_or(ProviderError::InvalidResponse)?;
    let (repository_id, pull_request_id, number) = change.ok_or(ProviderError::InvalidResponse)?;
    Ok(GiteaPullRequest {
        change: &delivery.change,
        reviewer_id,
        repository_id,
        repository_owner: &repository.owner,
        repository_name: &repository.name,
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
        && run.refs.forge == ForgeDialect::Gitea
        && run.object_format == ObjectFormat::Sha1
        && run.commits.candidate == delivery.provider_run.candidate_commit)
        .then_some(identity == delivery.provider_run)
        .ok_or(ProviderError::InvalidResponse)
}
