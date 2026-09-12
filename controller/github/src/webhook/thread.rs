use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

use super::comment::ReviewCommentRecord;
use super::pull::thread::ThreadPullRequest;
use super::pull::{PullRequestAccountKind, ReviewTeam, Team};
use super::repository::WorkflowOwner;
use super::{Absent, Installation};
use crate::owner::OwnerRecord;
use crate::pull::PullRefRecord;
use crate::repository::WorkflowRepositoryRecord;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReviewThreadEvent {
    Resolved {
        #[serde(flatten)]
        event: ThreadPayload,
    },
    Unresolved {
        #[serde(flatten)]
        event: ThreadPayload<
            UInt,
            WorkflowOwner,
            PullRefRecord<WorkflowRepositoryRecord<Option<OwnerRecord>>>,
            Team,
            String,
        >,
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
    deserialize = "OriginalLine: Deserialize<'de>, Account: Deserialize<'de>, Head: Deserialize<'de>, RequestedTeam: Deserialize<'de>, Title: Deserialize<'de>"
))]
pub struct ThreadPayload<
    OriginalLine = Nullable<UInt>,
    Account = WorkflowOwner<PullRequestAccountKind>,
    Head = PullRefRecord<Option<WorkflowRepositoryRecord<Option<OwnerRecord>>>>,
    RequestedTeam = ReviewTeam,
    Title = Option<String>,
> {
    pub thread: ReviewThread<ReviewCommentRecord<OriginalLine, Account>>,
    pub pull_request: ThreadPullRequest<Account, Head, RequestedTeam, Title>,
    pub repository: WorkflowRepositoryRecord,
    pub installation: Option<Installation>,
    pub number: Absent,
    pub issue: Absent,
    pub review: Absent,
    pub comment: Absent,
    pub check_run: Absent,
    pub check_suite: Absent,
    pub workflow: Absent,
    pub workflow_run: Absent,
    pub requested_action: Absent,
    pub changes: Absent,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ReviewThread<
    Comment = ReviewCommentRecord<Nullable<UInt>, WorkflowOwner<PullRequestAccountKind>>,
> {
    pub node_id: String,
    pub comments: Vec<Comment>,
}
