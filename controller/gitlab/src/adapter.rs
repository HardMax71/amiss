use std::sync::Arc;

use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, CheckConclusion, HandleOutcome,
    IngressCheck, PlanScope, ProviderAdapter, ProviderError, ProviderIdentity, ProviderNamespace,
    Publication, RelationStatusRecord, RelationStatusTarget, RelationSubject, RelationSubjectHead,
    VerifiedDelivery, relation_status_publication,
};
use amiss_wire::model::ObjectFormat;
use amiss_wire::relation::RelationSnapshot;

use crate::identity::{
    branch_ref, exact_sha1, parse_change_id, parse_delivery_id, parse_run_id, repository_identity,
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

    /// External verification, `None` from the default for an API
    /// without a verifier.
    ///
    /// # Errors
    ///
    /// No fact could be gathered before the first one.
    fn verify_external(
        &self,
        _plan: &amiss_wire::json::Value,
        _checked_at: &str,
    ) -> Result<Option<amiss_wire::json::Value>, ProviderError> {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct GitLabMergeTrainAdapter<A> {
    source: Arc<GitLabOidc>,
    api: A,
}

impl<A: GitLabApi> GitLabMergeTrainAdapter<A> {
    pub const fn new(source: Arc<GitLabOidc>, api: A) -> Self {
        Self { source, api }
    }

    /// Resolves the exact merge-train candidate represented by one active policy job.
    ///
    /// # Errors
    ///
    /// The subject is not this policy's project and protected target, or the
    /// authenticated job and train are no longer active.
    pub fn resolve_relation_head(
        &self,
        delivery: &AuthenticatedDelivery,
        subject: &RelationSubject,
    ) -> Result<RelationSubjectHead, ProviderError> {
        validate_relation_scope(
            &self.source,
            delivery,
            &subject.scope,
            subject.object_format,
        )?;
        if branch_ref(&self.source.policy.target_branch).as_ref() != Some(&subject.target) {
            return Err(ProviderError::InvalidResponse);
        }
        let current = policy_job_snapshot(&self.source, &self.api, delivery)?;
        (current.state == ChangeState::Active)
            .then(|| RelationSubjectHead {
                subject: subject.clone(),
                candidate: RelationSnapshot {
                    commit: current.run.commits.candidate,
                    tree: current.run.trees.candidate,
                },
            })
            .ok_or(ProviderError::AuthorizationRevoked)
    }

    /// Binds one staged relation result to the currently running policy job.
    ///
    /// The returned boolean is the HTTP success decision; this makes no
    /// provider write and cannot be resumed after the job stops.
    ///
    /// # Errors
    ///
    /// The status or target is malformed, names another policy job, or the
    /// authenticated job and train are no longer active.
    pub fn relation_policy_job_result(
        &self,
        delivery: &AuthenticatedDelivery,
        status: &RelationStatusRecord,
        target: &RelationStatusTarget,
    ) -> Result<bool, ProviderError> {
        validate_relation_scope(
            &self.source,
            delivery,
            &target.scope,
            target.candidate_commit.object_format(),
        )?;
        if target.required_status_name != self.source.policy.job_name
            || target.candidate_commit != delivery.provider_run.candidate_commit
        {
            return Err(ProviderError::InvalidResponse);
        }
        let publication = relation_status_publication(status, target)
            .map_err(|_defect| ProviderError::InvalidResponse)?;
        let current = policy_job_snapshot(&self.source, &self.api, delivery)?;
        (current.state == ChangeState::Active
            && current.run.commits.candidate == target.candidate_commit)
            .then_some(publication.passing)
            .ok_or(ProviderError::AuthorizationRevoked)
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
        policy_job_snapshot(&self.source, &self.api, delivery)
    }

    fn publish(
        &self,
        delivery: &AuthenticatedDelivery,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        if publication.provider_run != delivery.provider_run {
            return Err(ProviderError::InvalidResponse);
        }
        let current = policy_job_snapshot(&self.source, &self.api, delivery)?;
        let frozen = publication.run == current.run
            && publication.gate_commit == current.gate_commit
            && publication.gate_commit == delivery.provider_run.candidate_commit
            && conclusion_matches(current.state, publication.conclusion);
        frozen
            .then_some(())
            .ok_or(ProviderError::AuthorizationRevoked)
    }

    fn verify_external(
        &self,
        plan: &amiss_wire::json::Value,
        checked_at: &str,
    ) -> Result<Option<amiss_wire::json::Value>, ProviderError> {
        self.api.verify_external(plan, checked_at)
    }
}

fn policy_job_snapshot(
    source: &GitLabOidc,
    api: &impl GitLabApi,
    delivery: &AuthenticatedDelivery,
) -> Result<ChangeSnapshot, ProviderError> {
    let query = refresh_query(delivery, &source.provider, &source.policy)?;
    let refresh = api.refresh(&query)?;
    snapshot(delivery, &source.policy, &query, &refresh)
}

fn validate_relation_scope(
    source: &GitLabOidc,
    delivery: &AuthenticatedDelivery,
    scope: &PlanScope,
    object_format: ObjectFormat,
) -> Result<(), ProviderError> {
    (scope
        == &PlanScope {
            provider: delivery.identity.provider.clone(),
            integration: delivery.identity.integration.clone(),
            repository: delivery.change.repository.clone(),
        }
        && delivery.identity.provider == source.provider
        && object_format == ObjectFormat::Sha1)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)
}

pub fn policy_job_accepted(outcome: &HandleOutcome) -> bool {
    matches!(
        outcome,
        HandleOutcome::Published {
            conclusion: CheckConclusion::Pass,
            ..
        }
    )
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
