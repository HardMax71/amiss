mod tests;

use amiss_controller::{
    ArtifactAuditDigests, IntegrationId, PlanScope, ProviderError, RelationStatusRecord,
    RelationStatusTarget, RelationSubject, RelationSubjectHead,
};
use amiss_wire::controls::valid_required_status_name;
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::relation::RelationVerdict;

use super::Client;
use super::model::{CommitRecord, CreateCheckRun, CreateCheckRunOutput};
use super::publication::{CheckRunDecision, check_run_decision, validate_created};
use super::rest::GitHubRest;

const CHECK_RUN_DOMAIN: &str = "amiss/controller-github-relation-check-run-v1";
const COMPLETED: &str = "completed";
const TITLE: &str = "Amiss cross-repository relation";

pub(super) trait GitHubRelationRest {
    fn relation_head(
        &self,
        repository: &RepositoryIdentity,
        target: &BranchRef,
    ) -> Result<CommitRecord, ProviderError>;
}

impl<R: GitHubRelationRest> Client<R> {
    pub(super) fn resolve_relation_head(
        &self,
        subject: &RelationSubject,
    ) -> Result<RelationSubjectHead, ProviderError> {
        let repository = &subject.scope.repository;
        validate_relation_scope(&self.config, &subject.scope, subject.object_format)?;

        let head = self.rest.relation_head(repository, &subject.target)?;
        let candidate_commit =
            Oid::new(ObjectFormat::Sha1, head.sha).ok_or(ProviderError::InvalidResponse)?;
        Oid::new(ObjectFormat::Sha1, head.tree).ok_or(ProviderError::InvalidResponse)?;
        Ok(RelationSubjectHead {
            subject: subject.clone(),
            candidate_commit,
        })
    }
}

impl<R: GitHubRest> Client<R> {
    pub(super) fn publish_relation_status(
        &self,
        status: &RelationStatusRecord,
        target: &RelationStatusTarget,
    ) -> Result<(), ProviderError> {
        let expected = relation_check_run(&self.config, status, target)?;
        let deadline = self.rest.deadline()?;
        let runs = self.rest.check_runs(
            &target.scope.repository,
            &target.candidate_commit,
            self.config.app_id,
            &target.required_status_name,
            deadline,
        )?;
        match check_run_decision(&self.config, expected, &runs)? {
            CheckRunDecision::Reuse => Ok(()),
            CheckRunDecision::Create(expected) => {
                let created =
                    self.rest
                        .create_check_run(&target.scope.repository, &expected, deadline)?;
                validate_created(&self.config, &expected, &created)
            }
        }
    }
}

fn relation_check_run(
    config: &super::Config,
    status: &RelationStatusRecord,
    target: &RelationStatusTarget,
) -> Result<CreateCheckRun, ProviderError> {
    validate_relation_scope(
        config,
        &target.scope,
        target.candidate_commit.object_format(),
    )?;
    let destinations = &status.targets.destinations;
    let exact_target = destinations.iter().filter(|item| *item == target).count() == 1;
    let ordered_targets = (1..=2).contains(&destinations.len())
        && destinations
            .windows(2)
            .all(|pair| matches!(pair, [left, right] if left.role < right.role));
    let ArtifactAuditDigests::Relation(audit) = status.audit.audit else {
        return Err(ProviderError::InvalidResponse);
    };
    let artifact = &status.audit.artifact;
    if status.completed
        || !exact_target
        || !ordered_targets
        || !valid_required_status_name(&target.required_status_name)
        || artifact.report_digest != audit.report_digest
        || artifact.semantic_digest.is_some()
        || artifact.assessment_digest.is_some()
        || artifact.external_tally.is_some()
        || artifact.external_incomplete
        || (audit.verdict != RelationVerdict::Unproven && audit.evidence_digest.is_none())
    {
        return Err(ProviderError::InvalidResponse);
    }

    let verdict = audit.verdict.as_ref();
    let evidence = audit
        .evidence_digest
        .map_or_else(|| "none".to_owned(), |digest| digest.to_string());
    let repository = &target.scope.repository;
    let summary = format!(
        "relation: {}\ncoordination: {}\nfence: {}\nverdict: {verdict}\nprovider: {}/{}\nrepository: {}/{}/{}\ntrigger-role: {}\ndestination-role: {}\nstatus: {}\ncandidate-commit: {}\nreport: {}\nplan: {}\nevidence: {evidence}\nassessment: {}",
        status.targets.relation.as_str(),
        status.targets.coordination.as_str(),
        status.targets.fence.get(),
        target.scope.provider.namespace,
        target.scope.provider.instance,
        repository.host(),
        repository.owner(),
        repository.name(),
        status.targets.trigger_role.as_str(),
        target.role.as_str(),
        target.required_status_name,
        target.candidate_commit.as_str(),
        audit.report_digest,
        audit.plan_digest,
        audit.assessment_digest,
    );
    let conclusion = match audit.verdict {
        RelationVerdict::Aligned | RelationVerdict::ResolvedDrift => "success",
        RelationVerdict::IntroducedDrift
        | RelationVerdict::PreExistingDrift
        | RelationVerdict::Unproven => "failure",
    };
    Ok(CreateCheckRun {
        name: target.required_status_name.clone(),
        head_sha: target.candidate_commit.as_str().to_owned(),
        external_id: hb(CHECK_RUN_DOMAIN, summary.as_bytes()).to_string(),
        status: COMPLETED,
        conclusion: conclusion.to_owned(),
        output: CreateCheckRunOutput {
            title: TITLE.to_owned(),
            summary,
        },
    })
}

fn validate_relation_scope(
    config: &super::Config,
    scope: &PlanScope,
    object_format: ObjectFormat,
) -> Result<(), ProviderError> {
    let integration = IntegrationId::new(config.installation_id.to_string())
        .ok_or(ProviderError::InvalidResponse)?;
    (scope.provider == config.provider
        && scope.integration == integration
        && scope.repository.host() == config.provider.instance.as_str()
        && crate::acquisition::canonical_github_repository(&scope.repository)
        && object_format == ObjectFormat::Sha1)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)
}
