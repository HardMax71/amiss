use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize)]
pub(super) struct RepositoryRecord {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: OwnerRecord,
    pub default_branch: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct OwnerRecord {
    pub login: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct PullRequestRecord {
    pub id: u64,
    pub number: u64,
    pub state: String,
    pub mergeable: Option<bool>,
    pub merge_commit_sha: Option<String>,
    pub head: PullRefRecord,
    pub base: PullRefRecord,
}

#[derive(Clone, Deserialize)]
pub(super) struct PullRefRecord {
    pub sha: String,
    #[serde(rename = "ref")]
    pub branch: String,
    pub repo: Option<PullRepositoryRecord>,
}

#[derive(Clone, Deserialize)]
pub(super) struct PullRepositoryRecord {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: OwnerRecord,
}

#[derive(Clone)]
pub(super) struct CommitRecord {
    pub sha: String,
    pub tree: String,
}

#[derive(Deserialize)]
pub(super) struct RepositoryCommitRecord {
    pub sha: String,
    pub commit: RepositoryCommit,
}

#[derive(Deserialize)]
pub(super) struct RepositoryCommit {
    pub tree: GitObjectRecord,
}

#[derive(Deserialize)]
pub(super) struct GitCommitRecord {
    pub sha: String,
    pub tree: GitObjectRecord,
    pub parents: Vec<GitObjectRecord>,
}

#[derive(Deserialize)]
pub(super) struct GitObjectRecord {
    pub sha: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct BranchRule {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub parameters: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct RequiredStatusParameters {
    pub required_status_checks: Vec<RequiredStatus>,
    pub strict_required_status_checks_policy: bool,
}

#[derive(Deserialize)]
pub(super) struct RequiredStatus {
    pub context: String,
    pub integration_id: Option<u64>,
}

#[derive(Clone)]
pub(super) struct RefreshData {
    pub repository: RepositoryRecord,
    pub pull_request: PullRequestRecord,
    pub target: CommitRecord,
    pub candidate: CommitRecord,
    pub current_head: CommitRecord,
    pub gate: GateCommitRecord,
    pub rules: Vec<BranchRule>,
}

#[derive(Clone)]
pub(super) struct GateCommitRecord {
    pub sha: String,
    pub tree: String,
    pub parents: Vec<String>,
}

#[derive(Clone, Deserialize)]
pub(super) struct CheckRunRecord {
    pub id: u64,
    pub name: String,
    pub head_sha: String,
    pub external_id: Option<String>,
    pub status: String,
    pub conclusion: Option<String>,
    pub output: CheckRunOutputRecord,
    pub app: Option<CheckRunApp>,
}

#[derive(Clone, Deserialize)]
pub(super) struct CheckRunOutputRecord {
    pub title: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Deserialize)]
pub(super) struct CheckRunApp {
    pub id: u64,
}

#[derive(Deserialize)]
pub(super) struct CheckRunPage {
    pub total_count: u64,
    pub check_runs: Vec<CheckRunRecord>,
}

#[derive(Clone, Serialize)]
pub(super) struct CreateCheckRun {
    pub name: String,
    pub head_sha: String,
    pub external_id: String,
    pub status: &'static str,
    pub conclusion: String,
    pub output: CreateCheckRunOutput,
}

#[derive(Clone, Serialize)]
pub(super) struct CreateCheckRunOutput {
    pub title: String,
    pub summary: String,
}
