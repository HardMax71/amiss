mod tests;

use amiss_controller::{AcquiredSemanticTemplate, ProviderError, WorkflowArtifactExpectation};
use amiss_wire::digest::{Digest, sha256};
use amiss_wire::model::Oid;
use serde::{Deserialize, Serialize};

use super::model::OwnerRecord;
use super::{Config, refresh};

pub(super) const EXACT_PAGE_SIZE: u8 = 2;

#[derive(Clone, Deserialize)]
pub(super) struct WorkflowRepositoryRecord {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) full_name: String,
    pub(super) owner: OwnerRecord,
}

#[derive(Clone, Deserialize)]
pub(super) struct WorkflowRunRecord {
    pub(super) id: u64,
    pub(super) head_sha: String,
    pub(super) event: String,
    pub(super) status: String,
    pub(super) conclusion: Option<String>,
    pub(super) workflow_id: u64,
    pub(super) run_attempt: u64,
    pub(super) repository: WorkflowRepositoryRecord,
    pub(super) head_repository: WorkflowRepositoryRecord,
}

#[derive(Deserialize)]
pub(super) struct WorkflowRunPage {
    pub(super) total_count: u64,
    pub(super) workflow_runs: Vec<WorkflowRunRecord>,
}

#[derive(Clone, Deserialize)]
pub(super) struct ArtifactRunRecord {
    pub(super) id: u64,
    pub(super) repository_id: u64,
    pub(super) head_repository_id: u64,
    pub(super) head_sha: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct WorkflowArtifactRecord {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) size_in_bytes: u64,
    pub(super) expired: bool,
    pub(super) digest: String,
    pub(super) workflow_run: Option<ArtifactRunRecord>,
}

#[derive(Deserialize)]
pub(super) struct WorkflowArtifactPage {
    pub(super) total_count: u64,
    pub(super) artifacts: Vec<WorkflowArtifactRecord>,
}

#[derive(Serialize)]
pub(super) struct WorkflowRunQuery<'a> {
    pub(super) event: &'a str,
    pub(super) head_sha: &'a str,
    pub(super) status: &'static str,
    pub(super) exclude_pull_requests: bool,
    pub(super) per_page: u8,
    pub(super) page: u8,
}

#[derive(Serialize)]
pub(super) struct WorkflowArtifactQuery<'a> {
    pub(super) name: &'a str,
    pub(super) per_page: u8,
    pub(super) page: u8,
}

#[derive(Clone, Copy)]
pub(super) struct SelectedArtifact {
    pub(super) id: u64,
    pub(super) size: u64,
    pub(super) digest: Digest,
}

pub(super) fn validate_workflow_request(
    config: &Config,
    expectation: &WorkflowArtifactExpectation,
    candidate: &Oid,
) -> Result<(), ProviderError> {
    (config.provider == expectation.provider
        && crate::workflow_artifact::valid_github_expectation(expectation)
        && crate::acquisition::exact_sha1(candidate))
    .then_some(())
    .ok_or(ProviderError::InvalidResponse)
}

pub(super) fn select_workflow_run(
    config: &Config,
    expectation: &WorkflowArtifactExpectation,
    candidate: &Oid,
    page: WorkflowRunPage,
) -> Result<WorkflowRunRecord, ProviderError> {
    validate_workflow_request(config, expectation, candidate)?;
    let run = exactly_one(page.total_count, page.workflow_runs)?;
    let repository = refresh::repository_identity(
        config,
        &run.repository.owner.login,
        &run.repository.name,
        &run.repository.full_name,
    )?;
    refresh::repository_identity(
        config,
        &run.head_repository.owner.login,
        &run.head_repository.name,
        &run.head_repository.full_name,
    )?;
    let numeric_workflow_matches = expectation
        .workflow_identity
        .as_str()
        .parse::<u64>()
        .map_or(true, |workflow_id| workflow_id == run.workflow_id);
    let valid = run.id > 0
        && run.repository.id > 0
        && run.head_repository.id > 0
        && run.workflow_id > 0
        && run.run_attempt > 0
        && run.head_sha == candidate.as_str()
        && run.event == expectation.event.as_str()
        && run.status == "completed"
        && run.conclusion.as_deref() == Some("success")
        && repository == expectation.repository
        && numeric_workflow_matches;
    valid.then_some(run).ok_or(ProviderError::InvalidResponse)
}

pub(super) fn select_workflow_artifact(
    expectation: &WorkflowArtifactExpectation,
    run: &WorkflowRunRecord,
    page: WorkflowArtifactPage,
) -> Result<SelectedArtifact, ProviderError> {
    let artifact = exactly_one(page.total_count, page.artifacts)?;
    let linked = artifact
        .workflow_run
        .ok_or(ProviderError::InvalidResponse)?;
    let digest = Digest::from_wire(&artifact.digest).ok_or(ProviderError::InvalidResponse)?;
    if artifact.id == 0
        || artifact.name != expectation.artifact_name
        || !(1..=expectation.archive_byte_limit).contains(&artifact.size_in_bytes)
        || artifact.expired
        || linked.id != run.id
        || linked.repository_id != run.repository.id
        || linked.head_repository_id != run.head_repository.id
        || linked.head_sha != run.head_sha
    {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(SelectedArtifact {
        id: artifact.id,
        size: artifact.size_in_bytes,
        digest,
    })
}

fn exactly_one<T>(total_count: u64, records: Vec<T>) -> Result<T, ProviderError> {
    if total_count != 1 {
        return Err(ProviderError::InvalidResponse);
    }
    <[T; 1]>::try_from(records)
        .map(|[record]| record)
        .map_err(|_records| ProviderError::InvalidResponse)
}

pub(super) fn finish_workflow_artifact(
    expectation: &WorkflowArtifactExpectation,
    selected: SelectedArtifact,
    archive: &[u8],
) -> Result<AcquiredSemanticTemplate, ProviderError> {
    (u64::try_from(archive.len()) == Ok(selected.size) && sha256(archive) == selected.digest)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)?;
    crate::decode_workflow_artifact(expectation, archive)
        .map_err(|_defect| ProviderError::InvalidResponse)
}
