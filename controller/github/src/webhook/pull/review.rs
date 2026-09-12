use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use super::{LockReason, PullRequestAccountKind, Reviewer, Team};
use crate::owner::OwnerRecord;
use crate::pull::metadata::{
    AuthorAssociation, AutoMergeRecord, LabelRecord, MilestoneRecord, PullRequestLinks, StackRecord,
};
use crate::pull::{PullRefRecord, State};
use crate::repository::WorkflowRepositoryRecord;
use crate::webhook::repository::WorkflowOwner;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPullRequest {
    #[serde(flatten)]
    pub context: PullRequestContext,
    pub auto_merge: Nullable<Box<AutoMergeRecord<Option<WorkflowOwner>, Option<String>>>>,
    pub draft: bool,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestContext<
    Assignee = WorkflowOwner<PullRequestAccountKind>,
    Requested = Reviewer,
    Creator = WorkflowOwner<PullRequestAccountKind>,
    User = WorkflowOwner<PullRequestAccountKind>,
    Head = PullRefRecord<Option<WorkflowRepositoryRecord<Option<OwnerRecord>>>>,
> {
    pub id: u64,
    pub number: u64,
    #[serde(
        bound(deserialize = "Head: Deserialize<'de>"),
        deserialize_with = "Head::deserialize"
    )]
    pub head: Head,
    pub base: PullRefRecord<WorkflowRepositoryRecord<Option<OwnerRecord>>>,
    pub url: String,
    pub node_id: String,
    pub html_url: String,
    pub diff_url: String,
    pub patch_url: String,
    pub issue_url: String,
    pub state: State,
    pub locked: bool,
    pub title: String,
    pub user: Nullable<Box<User>>,
    pub body: Nullable<String>,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Nullable<String>,
    pub merged_at: Nullable<String>,
    pub merge_commit_sha: Nullable<Oid>,
    pub assignee: Nullable<Box<Assignee>>,
    pub assignees: Vec<Option<Assignee>>,
    pub requested_reviewers: Vec<Option<Requested>>,
    pub requested_teams: Vec<Team>,
    pub labels: Vec<LabelRecord>,
    pub milestone: Nullable<Box<MilestoneRecord<Creator>>>,
    pub commits_url: String,
    pub review_comments_url: String,
    pub review_comment_url: String,
    pub comments_url: String,
    pub statuses_url: String,
    #[serde(rename = "_links")]
    pub links: PullRequestLinks,
    pub author_association: AuthorAssociation,
    pub active_lock_reason: Nullable<LockReason>,
}

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewActivityPullRequest<
    Head = PullRefRecord<Option<WorkflowRepositoryRecord<Option<OwnerRecord>>>>,
> {
    #[serde(flatten)]
    pub request: PullRequestContext<
        WorkflowOwner<PullRequestAccountKind>,
        Reviewer,
        WorkflowOwner<PullRequestAccountKind>,
        WorkflowOwner<PullRequestAccountKind>,
        Head,
    >,
    pub auto_merge: Nullable<Box<AutoMergeRecord<Option<WorkflowOwner>, Option<String>>>>,
    pub draft: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub stack: Option<StackRecord>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, bound(deserialize = "Context: Deserialize<'de>"))]
pub struct CommentPullRequest<Context = PullRequestContext> {
    #[serde(flatten)]
    pub context: Context,
    pub draft: Option<bool>,
    pub auto_merge: Option<Nullable<AutoMergeRecord<Option<WorkflowOwner>, Option<String>>>>,
    pub stack: Option<StackRecord>,
}
