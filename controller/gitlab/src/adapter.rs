use std::sync::Arc;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, CheckConclusion, HandleOutcome, IngressCheck,
    ProviderAdapter, ProviderError, ProviderIdentity, ProviderNamespace, Publication,
    VerifiedDelivery,
};
use amiss_wire::model::ObjectFormat;

use crate::identity::{
    exact_sha1, parse_change_id, parse_delivery_id, parse_run_id, repository_identity,
};
use crate::snapshot::{conclusion_matches, snapshot};
use crate::{GitLabOidc, GitLabRefresh, GitLabRefreshQuery, PolicyBinding};

pub trait GitLabApi: Send + Sync {
    /// Reads the current provider state bound to one authenticated policy job.
    ///
    /// # Errors
    ///
    /// The exact job, pipeline, train, change, protection, or Git objects
    /// cannot be obtained.
    fn refresh(&self, query: &GitLabRefreshQuery) -> Result<GitLabRefresh, ProviderError>;
}

#[derive(Clone)]
pub struct GitLabMergeTrainAdapter<A> {
    source: Arc<GitLabOidc>,
    api: A,
}

impl<A> GitLabMergeTrainAdapter<A> {
    pub const fn new(source: Arc<GitLabOidc>, api: A) -> Self {
        Self { source, api }
    }
}

impl<A: GitLabApi> ProviderAdapter for GitLabMergeTrainAdapter<A> {
    fn namespace(&self) -> &ProviderNamespace {
        &self.source.provider.namespace
    }

    fn authenticate(&self, check: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        self.source.authenticate(check)
    }

    fn refresh(&self, delivery: &AuthenticatedDelivery) -> Result<ChangeSnapshot, ProviderError> {
        let query = refresh_query(delivery, &self.source.provider, &self.source.policy)?;
        let refresh = self.api.refresh(&query)?;
        snapshot(delivery, &self.source.policy, &query, &refresh)
    }

    fn publish(
        &self,
        delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        if publication.provider_run != delivery.provider_run {
            return Err(ProviderError::InvalidResponse);
        }
        let query = refresh_query(delivery, &self.source.provider, &self.source.policy)?;
        let refresh = self.api.refresh(&query)?;
        let current = snapshot(delivery, &self.source.policy, &query, &refresh)?;
        let frozen = publication.run == current.run
            && publication.gate_commit == current.gate_commit
            && publication.gate_commit == delivery.provider_run.candidate_commit
            && conclusion_matches(current.state, publication.conclusion);
        frozen
            .then_some(())
            .ok_or(ProviderError::AuthorizationRevoked)
    }
}

pub fn policy_job_accepted(outcome: &HandleOutcome) -> bool {
    matches!(outcome, HandleOutcome::Published(CheckConclusion::Pass))
}

fn refresh_query(
    delivery: &AuthenticatedDelivery,
    provider: &ProviderIdentity,
    policy: &PolicyBinding,
) -> Result<GitLabRefreshQuery, ProviderError> {
    let (project_id, merge_request_iid) =
        parse_change_id(delivery.change.change.as_str()).ok_or(ProviderError::InvalidResponse)?;
    let (pipeline_id, job_id) = parse_run_id(delivery.provider_run.run_id.as_str())
        .ok_or(ProviderError::InvalidResponse)?;
    let runner_id = parse_delivery_id(delivery.identity.delivery.as_str())
        .ok_or(ProviderError::InvalidResponse)?;
    let expected_repository = repository_identity(provider.instance.as_str(), &policy.project_path)
        .ok_or(ProviderError::InvalidResponse)?;
    let exact_gate = exact_oid(delivery.provider_run.candidate_commit.as_str())?;
    let valid = delivery.identity.provider == *provider
        && delivery.change.provider == *provider
        && delivery.identity.integration == policy.integration
        && delivery.change.repository == expected_repository
        && project_id == policy.project_id
        && delivery.provider_run.attempt.get() == 1
        && delivery.provider_run.object_format == ObjectFormat::Sha1
        && exact_gate == delivery.provider_run.candidate_commit;
    valid
        .then_some(GitLabRefreshQuery {
            project_id,
            merge_request_iid,
            pipeline_id,
            job_id,
            runner_id,
            gate_commit: exact_gate,
        })
        .ok_or(ProviderError::InvalidResponse)
}

fn exact_oid(raw: &str) -> Result<amiss_wire::model::Oid, ProviderError> {
    exact_sha1(raw).ok_or(ProviderError::InvalidResponse)
}
