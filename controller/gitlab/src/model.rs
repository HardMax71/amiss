use std::time::Duration;

use amiss_wire::model::Oid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabRefreshQuery {
    pub project_id: u64,
    pub merge_request_iid: u64,
    pub pipeline_id: u64,
    pub job_id: u64,
    pub runner_id: u64,
    pub gate_commit: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabRefresh {
    pub project: GitLabProject,
    pub job: GitLabJob,
    pub pipeline: GitLabPipeline,
    pub train: Option<GitLabTrainCar>,
    pub merge_request: GitLabMergeRequest,
    pub target: GitLabBranch,
    pub gate: GitLabCommit,
    pub base: GitLabCommit,
    pub protections: Vec<GitLabProtection>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabProject {
    pub id: u64,
    pub path_with_namespace: String,
    pub default_branch: String,
    pub http_url_to_repo: String,
    pub repository_object_format: String,
    pub checks: GitLabMergeChecks,
    pub train: GitLabTrainSettings,
    pub merge_method: String,
    pub squash_option: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabMergeChecks {
    pub pipeline_must_succeed: bool,
    pub skipped_pipeline_allowed: bool,
    pub merged_results_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabTrainSettings {
    pub enabled: bool,
    pub skip_allowed: bool,
    pub enforcement: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabJob {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub source: String,
    pub pipeline_id: u64,
    pub commit: String,
    pub runner_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabPipeline {
    pub id: u64,
    pub project_id: u64,
    pub sha: String,
    pub reference: String,
    pub source: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabTrainCar {
    pub id: u64,
    pub status: String,
    pub target_branch: String,
    pub merge_request_iid: u64,
    pub merge_request_project_id: u64,
    pub merge_request_state: String,
    pub pipeline_id: u64,
    pub pipeline_project_id: u64,
    pub pipeline_sha: String,
    pub pipeline_ref: String,
    pub pipeline_source: String,
    pub pipeline_status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct GitLabMergeRequest {
    pub iid: u64,
    pub project_id: u64,
    pub state: String,
    pub draft: bool,
    pub source_project_id: u64,
    pub target_project_id: u64,
    pub source_branch: String,
    pub target_branch: String,
    pub sha: String,
    pub detailed_merge_status: String,
    pub squash_on_merge: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabBranch {
    pub name: String,
    pub commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabCommit {
    pub id: String,
    pub tree: String,
    pub parents: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct GitLabProtection {
    pub name: String,
    pub allow_force_push: bool,
    #[serde(default)]
    pub push_access_levels: Vec<GitLabAccess>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct GitLabAccess {
    pub access_level: u64,
    pub user_id: Option<u64>,
    pub group_id: Option<u64>,
    pub deploy_key_id: Option<u64>,
    pub member_role_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabObjectRequest {
    pub project_id: u64,
    pub repository_url: String,
    pub gate_commit: Oid,
    pub base_commit: Oid,
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabObjects {
    pub gate: GitLabCommit,
    pub base: GitLabCommit,
}
