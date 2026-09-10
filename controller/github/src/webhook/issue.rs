use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

use super::app::WebhookApp;
use super::comment::Reactions;
use super::pull::{LockReason, PullRequestAccountKind};
use super::repository::WorkflowOwner;
use crate::pull::metadata::{AuthorAssociation, MilestoneRecord};
use context::CommentIssueContext;
use metadata::{IssueDependenciesSummary, IssuePullRequest, IssueType, SubIssuesSummary};

pub mod context;
pub mod field;
pub mod metadata;

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueRecord<Context = CommentIssueContext> {
    pub url: String,
    pub repository_url: String,
    pub labels_url: String,
    pub comments_url: String,
    pub events_url: String,
    pub html_url: String,
    pub id: UInt,
    pub node_id: String,
    pub number: UInt,
    pub title: String,
    #[serde(flatten)]
    pub context: Context,
    pub assignees: Vec<Option<WorkflowOwner<PullRequestAccountKind>>>,
    pub milestone: Nullable<Box<MilestoneRecord<WorkflowOwner<PullRequestAccountKind>>>>,
    pub comments: UInt,
    pub created_at: String,
    pub updated_at: String,
    pub closed_at: Nullable<String>,
    pub author_association: AuthorAssociation,
    pub active_lock_reason: Nullable<LockReason>,
    pub body: Nullable<String>,
    pub reactions: Reactions,
    pub draft: Option<bool>,
    pub performed_via_github_app: Option<Nullable<Box<WebhookApp>>>,
    pub pull_request: Option<IssuePullRequest>,
    pub sub_issues_summary: Option<SubIssuesSummary>,
    pub issue_dependencies_summary: Option<IssueDependenciesSummary>,
    pub state_reason: Option<Nullable<String>>,
    pub timeline_url: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<Nullable<IssueType>>,
}
