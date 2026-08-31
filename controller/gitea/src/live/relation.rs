use amiss_controller::{
    IntegrationId, PlanScope, ProviderError, RelationStatusRecord, RelationStatusTarget,
    relation_status_publication,
};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::ObjectFormat;

use super::model::{CommitStatusRecord, CreateCommitStatus};
use super::refresh::validate_reviewer;
use super::rest::GiteaRest;
use super::{Client, Config};

const STATUS_DOMAIN: &str = "amiss/controller-gitea-relation-status-v1";
pub(super) const MARKER: &str = "amiss-relation-v1: ";

pub(super) enum StatusDecision {
    Reuse,
    Create(CreateCommitStatus),
}

impl<R: GiteaRest> Client<R> {
    pub(super) fn publish_relation_status(
        &self,
        status: &RelationStatusRecord,
        target: &RelationStatusTarget,
    ) -> Result<(), ProviderError> {
        validate_relation_scope(
            &self.config,
            &target.scope,
            target.candidate_commit.object_format(),
        )?;
        let expected = relation_commit_status(status, target)?;
        let deadline = self.rest.deadline()?;
        validate_reviewer(&self.config, &self.rest.current_user(deadline)?)?;
        let statuses = self.rest.commit_statuses(
            &target.scope.repository,
            &target.candidate_commit,
            deadline,
        )?;
        match status_decision(&self.config, expected, &statuses)? {
            StatusDecision::Reuse => Ok(()),
            StatusDecision::Create(expected) => {
                let created = self.rest.create_commit_status(
                    &target.scope.repository,
                    &target.candidate_commit,
                    &expected,
                    deadline,
                )?;
                validate_created(&self.config, &expected, &created)
            }
        }
    }
}

pub(super) fn relation_commit_status(
    status: &RelationStatusRecord,
    target: &RelationStatusTarget,
) -> Result<CreateCommitStatus, ProviderError> {
    let publication = relation_status_publication(status, target)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    Ok(CreateCommitStatus {
        state: if publication.passing {
            "success".to_owned()
        } else {
            "failure".to_owned()
        },
        target_url: String::new(),
        description: format!(
            "{MARKER}{}",
            hb(STATUS_DOMAIN, publication.summary.as_bytes())
        ),
        context: target.required_status_name.clone(),
    })
}

pub(super) fn status_decision(
    config: &Config,
    expected: CreateCommitStatus,
    statuses: &[CommitStatusRecord],
) -> Result<StatusDecision, ProviderError> {
    let Some(latest) = statuses
        .iter()
        .find(|status| status.context == expected.context)
    else {
        return Ok(StatusDecision::Create(expected));
    };
    if matches_expected(config, &expected, latest) {
        return Ok(StatusDecision::Reuse);
    }
    if latest.description == expected.description || !owned_relation_status(config, latest) {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(StatusDecision::Create(expected))
}

pub(super) fn validate_created(
    config: &Config,
    expected: &CreateCommitStatus,
    created: &CommitStatusRecord,
) -> Result<(), ProviderError> {
    matches_expected(config, expected, created)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)
}

fn matches_expected(
    config: &Config,
    expected: &CreateCommitStatus,
    actual: &CommitStatusRecord,
) -> bool {
    owned_reviewer(config, actual)
        && actual.id > 0
        && actual.status == expected.state
        && actual.target_url == expected.target_url
        && actual.description == expected.description
        && actual.context == expected.context
}

fn owned_relation_status(config: &Config, status: &CommitStatusRecord) -> bool {
    owned_reviewer(config, status)
        && status.id > 0
        && status.target_url.is_empty()
        && matches!(status.status.as_str(), "success" | "failure")
        && status
            .description
            .strip_prefix(MARKER)
            .and_then(Digest::from_wire)
            .is_some()
}

fn owned_reviewer(config: &Config, status: &CommitStatusRecord) -> bool {
    status.creator.as_ref().is_some_and(|creator| {
        creator.id == config.reviewer.id
            && creator.login.eq_ignore_ascii_case(&config.reviewer.login)
    })
}

fn validate_relation_scope(
    config: &Config,
    scope: &PlanScope,
    object_format: ObjectFormat,
) -> Result<(), ProviderError> {
    let integration =
        IntegrationId::new(config.reviewer.id.to_string()).ok_or(ProviderError::InvalidResponse)?;
    let repository = &scope.repository;
    (scope.provider == config.provider
        && scope.integration == integration
        && repository.host() == config.provider.instance.as_str()
        && crate::identity::canonical_segment(repository.owner()).as_deref()
            == Some(repository.owner())
        && crate::identity::canonical_segment(repository.name()).as_deref()
            == Some(repository.name())
        && object_format == ObjectFormat::Sha1)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)
}
