use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use super::super::repository::WorkflowOwner;
use super::super::repository::pull::PullRepository;
use super::{LockReason, PullRequestAccountKind, Reviewer, Team};
use crate::pull::metadata::{
    AuthorAssociation, AutoMergeRecord, LabelRecord, MilestoneRecord, PullRequestLinks, StackRecord,
};
use crate::pull::{PullRefRecord, PullRequestRecord, State};
use crate::repository::metadata::{
    MergeCommitMessage, MergeCommitTitle, SquashMergeCommitMessage, SquashMergeCommitTitle,
};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestWebhook {
    #[serde(flatten)]
    pub request: PullRequestRecord,
    pub allow_auto_merge: Option<bool>,
    pub allow_update_branch: Option<bool>,
    pub delete_branch_on_merge: Option<bool>,
    pub merge_commit_message: Option<MergeCommitMessage>,
    pub merge_commit_title: Option<MergeCommitTitle>,
    pub squash_merge_commit_message: Option<SquashMergeCommitMessage>,
    pub squash_merge_commit_title: Option<SquashMergeCommitTitle>,
    pub use_squash_pr_title_as_default: Option<bool>,
}

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    Option<_> => #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")],
)]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SynchronizePullRequest {
    pub id: u64,
    pub number: u64,
    pub head: PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>>,
    pub base: PullRefRecord<Option<WorkflowOwner>, PullRepository>,
    pub url: String,
    pub node_id: String,
    pub html_url: String,
    pub diff_url: String,
    pub patch_url: String,
    pub issue_url: String,
    pub state: State,
    pub locked: bool,
    pub title: String,
    pub user: Nullable<Box<WorkflowOwner<PullRequestAccountKind>>>,
    pub body: Nullable<String>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Nullable<String>,
    pub merged_at: Nullable<String>,
    pub merge_commit_sha: Nullable<Oid>,
    pub assignee: Nullable<Box<WorkflowOwner<PullRequestAccountKind>>>,
    pub assignees: Vec<Option<WorkflowOwner<PullRequestAccountKind>>>,
    pub requested_reviewers: Vec<Option<Reviewer>>,
    pub requested_teams: Vec<Team>,
    pub labels: Vec<LabelRecord>,
    pub milestone: Nullable<Box<MilestoneRecord<WorkflowOwner<PullRequestAccountKind>>>>,
    pub commits_url: String,
    pub review_comments_url: String,
    pub review_comment_url: String,
    pub comments_url: String,
    pub statuses_url: String,
    #[serde(rename = "_links")]
    pub links: PullRequestLinks,
    pub author_association: AuthorAssociation,
    pub auto_merge: Nullable<Box<AutoMergeRecord<Option<WorkflowOwner>, Option<String>>>>,
    pub active_lock_reason: Nullable<LockReason>,
    pub draft: bool,
    pub merged: Option<Nullable<bool>>,
    pub mergeable: Option<Nullable<bool>>,
    pub rebaseable: Option<Nullable<bool>>,
    pub mergeable_state: Option<String>,
    pub merged_by: Option<Nullable<Box<WorkflowOwner>>>,
    pub comments: Option<UInt>,
    pub review_comments: Option<UInt>,
    pub maintainer_can_modify: Option<bool>,
    pub commits: Option<UInt>,
    pub additions: Option<UInt>,
    pub deletions: Option<UInt>,
    pub changed_files: Option<UInt>,
    pub stack: Option<StackRecord>,
}
