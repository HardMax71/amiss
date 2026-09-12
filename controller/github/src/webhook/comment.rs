use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::pull::review::{CommentPullRequest, PullRequestContext};
use super::pull::{PullRequestAccountKind, Reviewer, Team};
use super::repository::WorkflowOwner;
use super::review::ReviewChanges;
use super::{Absent, Installation};
use crate::pull::metadata::{AuthorAssociation, Link};
use crate::repository::WorkflowRepositoryRecord;

pub mod issue;

#[serde_with::apply(Absent => #[serde(default, skip_serializing)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReviewCommentEvent {
    Created {
        changes: Absent,
        #[serde(flatten)]
        event: CommentPayload,
    },
    Edited {
        changes: ReviewChanges,
        #[serde(flatten)]
        event: CommentPayload<
            WorkflowOwner,
            Reviewer<WorkflowOwner, Team>,
            WorkflowOwner<PullRequestAccountKind>,
            UInt,
        >,
    },
    Deleted {
        changes: Absent,
        #[serde(flatten)]
        event: CommentPayload<WorkflowOwner, Reviewer<WorkflowOwner, Team>, WorkflowOwner, UInt>,
    },
}

#[serde_with::apply(
    Option<_> => #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )],
    Absent => #[serde(default, skip_serializing)]
)]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(
    deserialize = "Assignee: Deserialize<'de>, Requested: Deserialize<'de>, Creator: Deserialize<'de>, OriginalLine: Deserialize<'de>"
))]
pub struct CommentPayload<
    Assignee = WorkflowOwner<PullRequestAccountKind>,
    Requested = Reviewer,
    Creator = WorkflowOwner<PullRequestAccountKind>,
    OriginalLine = Nullable<UInt>,
> {
    pub comment: ReviewCommentRecord<OriginalLine>,
    pub pull_request: CommentPullRequest<PullRequestContext<Assignee, Requested, Creator>>,
    pub repository: WorkflowRepositoryRecord,
    pub installation: Option<Installation>,
    pub number: Absent,
    pub issue: Absent,
    pub review: Absent,
    pub thread: Absent,
    pub check_run: Absent,
    pub check_suite: Absent,
    pub workflow: Absent,
    pub workflow_run: Absent,
    pub requested_action: Absent,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewCommentRecord<OriginalLine = UInt, User = WorkflowOwner> {
    pub url: String,
    pub pull_request_review_id: Nullable<UInt>,
    pub id: UInt,
    pub node_id: String,
    pub diff_hunk: String,
    pub path: String,
    pub position: Nullable<UInt>,
    pub original_position: UInt,
    pub commit_id: Oid,
    pub original_commit_id: Oid,
    pub user: Nullable<User>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub html_url: String,
    pub pull_request_url: String,
    pub author_association: AuthorAssociation,
    #[serde(rename = "_links")]
    pub links: CommentLinks,
    pub start_line: Nullable<UInt>,
    pub original_start_line: Nullable<UInt>,
    pub original_line: OriginalLine,
    pub line: Nullable<UInt>,
    pub start_side: Nullable<Side>,
    pub side: Side,
    pub reactions: Reactions,
    pub in_reply_to_id: Option<UInt>,
    pub subject_type: Option<SubjectType>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommentLinks {
    #[serde(rename = "self")]
    pub comment: Link,
    pub html: Link,
    pub pull_request: Link,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reactions {
    pub url: String,
    pub total_count: UInt,
    #[serde(rename = "+1")]
    pub thumbs_up: UInt,
    #[serde(rename = "-1")]
    pub thumbs_down: UInt,
    pub laugh: UInt,
    pub confused: UInt,
    pub heart: UInt,
    pub hooray: UInt,
    pub eyes: UInt,
    pub rocket: UInt,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "UPPERCASE")]
pub enum Side {
    Left,
    Right,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum SubjectType {
    Line,
    File,
}
