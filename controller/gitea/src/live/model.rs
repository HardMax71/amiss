use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
pub(super) struct UserRecord {
    pub id: u64,
    pub login: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct RepositoryRecord {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: UserRecord,
    pub default_branch: String,
    pub object_format_name: String,
    pub allow_manual_merge: Option<bool>,
}

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
    pub repo: Option<PullRepositoryRecord>,
}

#[derive(Clone, Deserialize)]
pub(super) struct PullRepositoryRecord {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: UserRecord,
}

#[derive(Clone, Deserialize)]
pub(super) struct BranchRecord {
    pub name: String,
    pub commit: Option<PayloadCommitRecord>,
    pub protected: bool,
    pub required_approvals: i64,
    pub effective_branch_protection_name: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct PayloadCommitRecord {
    pub id: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct BranchProtectionRecord {
    pub rule_name: String,
    #[serde(flatten)]
    pub writes: WriteProtection,
    #[serde(flatten)]
    pub force: ForceProtection,
    #[serde(flatten)]
    pub bypass: BypassProtection,
    #[serde(flatten)]
    pub approvals: ApprovalProtection,
    #[serde(flatten)]
    pub reviews: ReviewProtection,
    #[serde(flatten)]
    pub overrides: OverrideProtection,
}

#[derive(Clone, Deserialize)]
pub(super) struct WriteProtection {
    pub enable_push: bool,
    pub enable_push_whitelist: bool,
    pub push_whitelist_usernames: Vec<String>,
    pub push_whitelist_teams: Vec<String>,
    pub push_whitelist_deploy_keys: bool,
    #[serde(rename = "protected_file_patterns")]
    pub _protected_file_patterns: String,
    pub unprotected_file_patterns: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct ForceProtection {
    pub enable_force_push: Option<bool>,
    pub enable_force_push_allowlist: Option<bool>,
    pub force_push_allowlist_usernames: Option<Vec<String>>,
    pub force_push_allowlist_teams: Option<Vec<String>>,
    pub force_push_allowlist_deploy_keys: Option<bool>,
}

#[derive(Clone, Deserialize)]
pub(super) struct BypassProtection {
    pub enable_bypass_allowlist: Option<bool>,
    pub bypass_allowlist_usernames: Option<Vec<String>>,
    pub bypass_allowlist_teams: Option<Vec<String>>,
}

#[derive(Clone, Deserialize)]
pub(super) struct ApprovalProtection {
    pub required_approvals: i64,
    pub enable_approvals_whitelist: bool,
    #[serde(rename = "approvals_whitelist_username")]
    pub approvals_whitelist_usernames: Vec<String>,
    pub approvals_whitelist_teams: Vec<String>,
}

#[derive(Clone, Deserialize)]
pub(super) struct ReviewProtection {
    pub block_on_rejected_reviews: bool,
    pub block_on_outdated_branch: bool,
    pub dismiss_stale_approvals: bool,
}

#[derive(Clone, Deserialize)]
pub(super) struct OverrideProtection {
    pub ignore_stale_approvals: bool,
    pub block_admin_merge_override: Option<bool>,
    pub apply_to_admins: Option<bool>,
}

#[derive(Clone, Deserialize)]
pub(super) struct CommitRecord {
    pub sha: String,
    pub commit: Option<RepositoryCommitRecord>,
}

#[derive(Clone, Deserialize)]
pub(super) struct RepositoryCommitRecord {
    pub tree: Option<CommitMetaRecord>,
}

#[derive(Clone, Deserialize)]
pub(super) struct CommitMetaRecord {
    pub sha: String,
}

#[derive(Clone, Deserialize)]
pub(super) struct ReviewRecord {
    pub id: u64,
    pub user: Option<UserRecord>,
    pub state: String,
    pub body: String,
    pub commit_id: String,
    pub stale: bool,
    pub dismissed: bool,
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

#[derive(Clone, Serialize)]
pub(super) struct CreateReview {
    pub event: String,
    pub body: String,
    pub commit_id: String,
    pub comments: Vec<CreateReviewComment>,
}

#[derive(Clone, Serialize)]
pub(super) struct CreateReviewComment {}
