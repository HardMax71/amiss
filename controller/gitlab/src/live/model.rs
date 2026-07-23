use amiss_controller::ProviderError;
use serde::Deserialize;

use crate::{
    GitLabBranch, GitLabJob, GitLabMergeChecks, GitLabPipeline, GitLabProject, GitLabTrainCar,
    GitLabTrainSettings,
};

#[derive(Deserialize)]
pub(super) struct JobResponse {
    id: u64,
    name: String,
    status: String,
    source: String,
    commit: CommitReference,
    pipeline: PipelineReference,
    runner: Option<RunnerReference>,
}

#[derive(Deserialize)]
struct CommitReference {
    id: String,
}

#[derive(Deserialize)]
struct PipelineReference {
    id: u64,
}

#[derive(Deserialize)]
struct RunnerReference {
    id: u64,
}

#[derive(Deserialize)]
pub(super) struct PipelineResponse {
    id: u64,
    project_id: u64,
    sha: String,
    #[serde(rename = "ref")]
    reference: String,
    source: String,
    status: String,
}

#[derive(Deserialize)]
pub(super) struct TrainResponse {
    id: u64,
    status: String,
    target_branch: String,
    merge_request: TrainMergeRequest,
    pipeline: Option<PipelineResponse>,
}

#[derive(Deserialize)]
struct TrainMergeRequest {
    iid: u64,
    project_id: u64,
    state: String,
}

#[derive(Deserialize)]
pub(super) struct ProjectResponse {
    id: u64,
    path_with_namespace: String,
    default_branch: String,
    http_url_to_repo: String,
    repository_object_format: String,
    #[serde(flatten)]
    checks: ProjectChecksResponse,
    #[serde(flatten)]
    train: ProjectTrainResponse,
    merge_method: String,
    squash_option: String,
}

#[derive(Deserialize)]
struct ProjectChecksResponse {
    only_allow_merge_if_pipeline_succeeds: bool,
    allow_merge_on_skipped_pipeline: bool,
    merge_pipelines_enabled: bool,
}

#[derive(Deserialize)]
struct ProjectTrainResponse {
    #[serde(rename = "merge_trains_enabled")]
    enabled: bool,
    #[serde(rename = "merge_trains_skip_train_allowed")]
    skip_allowed: bool,
    #[serde(rename = "merge_train_enforcement")]
    enforcement: String,
}

#[derive(Deserialize)]
pub(super) struct BranchResponse {
    name: String,
    commit: CommitReference,
}

#[derive(Deserialize)]
pub(super) struct CommitResponse {
    pub id: String,
    pub parent_ids: Vec<String>,
}

pub(super) fn job(raw: JobResponse) -> Result<GitLabJob, ProviderError> {
    let runner_id = raw
        .runner
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?
        .id;
    Ok(job_record(raw, runner_id))
}

fn job_record(raw: JobResponse, runner_id: u64) -> GitLabJob {
    GitLabJob {
        id: raw.id,
        name: raw.name,
        status: raw.status,
        source: raw.source,
        pipeline_id: raw.pipeline.id,
        commit: raw.commit.id,
        runner_id,
    }
}

pub(super) fn pipeline(raw: PipelineResponse) -> GitLabPipeline {
    GitLabPipeline {
        id: raw.id,
        project_id: raw.project_id,
        sha: raw.sha,
        reference: raw.reference,
        source: raw.source,
        status: raw.status,
    }
}

pub(super) fn train(raw: TrainResponse) -> Result<GitLabTrainCar, ProviderError> {
    let pipeline = pipeline(raw.pipeline.ok_or(ProviderError::InvalidResponse)?);
    Ok(GitLabTrainCar {
        id: raw.id,
        status: raw.status,
        target_branch: raw.target_branch,
        merge_request_iid: raw.merge_request.iid,
        merge_request_project_id: raw.merge_request.project_id,
        merge_request_state: raw.merge_request.state,
        pipeline_id: pipeline.id,
        pipeline_project_id: pipeline.project_id,
        pipeline_sha: pipeline.sha,
        pipeline_ref: pipeline.reference,
        pipeline_source: pipeline.source,
        pipeline_status: pipeline.status,
    })
}

pub(super) fn project(raw: ProjectResponse) -> GitLabProject {
    GitLabProject {
        id: raw.id,
        path_with_namespace: raw.path_with_namespace,
        default_branch: raw.default_branch,
        http_url_to_repo: raw.http_url_to_repo,
        repository_object_format: raw.repository_object_format,
        checks: GitLabMergeChecks {
            pipeline_must_succeed: raw.checks.only_allow_merge_if_pipeline_succeeds,
            skipped_pipeline_allowed: raw.checks.allow_merge_on_skipped_pipeline,
            merged_results_enabled: raw.checks.merge_pipelines_enabled,
        },
        train: GitLabTrainSettings {
            enabled: raw.train.enabled,
            skip_allowed: raw.train.skip_allowed,
            enforcement: raw.train.enforcement,
        },
        merge_method: raw.merge_method,
        squash_option: raw.squash_option,
    }
}

pub(super) fn branch(raw: BranchResponse) -> GitLabBranch {
    GitLabBranch {
        name: raw.name,
        commit: raw.commit.id,
    }
}
