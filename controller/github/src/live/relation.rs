mod tests;

use amiss_controller::{
    IntegrationId, PlanScope, ProviderError, RelationStatusRecord, RelationStatusTarget,
    RelationSubject, RelationSubjectHead, relation_status_publication,
};
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepositoryIdentity};

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
    let publication = relation_status_publication(status, target)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    Ok(CreateCheckRun {
        name: target.required_status_name.clone(),
        head_sha: target.candidate_commit.as_str().to_owned(),
        external_id: hb(CHECK_RUN_DOMAIN, publication.summary.as_bytes()).to_string(),
        status: COMPLETED,
        conclusion: if publication.passing {
            "success".to_owned()
        } else {
            "failure".to_owned()
        },
        output: CreateCheckRunOutput {
            title: TITLE.to_owned(),
            summary: publication.summary,
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
