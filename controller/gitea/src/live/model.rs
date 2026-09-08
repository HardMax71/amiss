pub(super) use crate::branch::BranchRecord;
pub(super) use crate::commit::CommitRecord;
pub(super) use crate::protection::BranchProtectionRecord;
pub(super) use crate::pull::PullRequestRecord;
pub(super) use crate::reference::RefRecord;
pub(super) use crate::repository::RepositoryRecord;
pub(super) use crate::review::{CreateReview, ReviewRecord};
pub(super) use crate::status::{CommitStatusRecord, CreateCommitStatus};
pub(super) use crate::user::UserRecord;

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
