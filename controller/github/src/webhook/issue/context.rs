use amiss_wire::assessment::Nullable;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

use super::field::IssueFieldValue;
use crate::owner::OwnerRecord;
use crate::pull::State;
use crate::pull::metadata::LabelRecord;
use crate::webhook::comment::issue::{IssueCommentRecord, PinnedIssueCommentMetadata};
use crate::webhook::pull::PullRequestAccountKind;
use crate::webhook::repository::WorkflowOwner;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommentIssueContext {
    pub user: WorkflowOwner<PullRequestAccountKind>,
    pub labels: Vec<LabelRecord>,
    pub state: State,
    pub locked: bool,
    pub assignee: Nullable<WorkflowOwner<PullRequestAccountKind>>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct IssueActivityContext {
    pub user: Nullable<WorkflowOwner<PullRequestAccountKind>>,
    pub labels: Option<Vec<LabelRecord>>,
    pub state: Option<State>,
    pub locked: Option<bool>,
    pub assignee: Option<Nullable<WorkflowOwner<PullRequestAccountKind>>>,
    pub pinned_comment:
        Option<Nullable<Box<IssueCommentRecord<OwnerRecord, PinnedIssueCommentMetadata>>>>,
    pub issue_field_values: Option<Vec<IssueFieldValue>>,
}
