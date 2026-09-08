use serde::Deserialize;

pub(super) use crate::branch::BranchRecord;
pub(super) use crate::commit::CommitRecord;
pub(super) use crate::protection::BranchProtectionRecord;
pub(super) use crate::reference::RefRecord;
pub(super) use crate::repository::RepositoryRecord;
pub(super) use crate::review::{CreateReview, ReviewRecord};
pub(super) use crate::status::{CommitStatusRecord, CreateCommitStatus};
pub(super) use crate::user::UserRecord;

#[derive(Clone, Deserialize)]
pub(super) struct PullRequestRecord {
    pub id: u64,
    pub number: u64,
    pub state: String,
    pub mergeable: bool,
    pub merged: bool,
    pub merge_base: String,
    pub head: PullRefRecord,
    pub base: PullRefRecord,
}

#[derive(Clone, Deserialize)]
pub(super) struct PullRefRecord {
    pub sha: String,
    #[serde(rename = "ref")]
    pub branch: String,
    pub repo_id: u64,
    pub repo: Option<RepositoryRecord>,
}

#[derive(Clone)]
pub(super) struct RefreshData {
    pub reviewer: UserRecord,
    pub repository: RepositoryRecord,
    pub pull_request: PullRequestRecord,
    pub target_branch: BranchRecord,
    pub protection: BranchProtectionRecord,
    pub target: CommitRecord,
    pub candidate: CommitRecord,
    pub current_head: CommitRecord,
    pub reviews: Vec<ReviewRecord>,
}
