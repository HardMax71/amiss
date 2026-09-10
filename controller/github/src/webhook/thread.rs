use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

use super::comment::ReviewCommentRecord;
use super::pull::thread::ThreadPullRequest;
use super::pull::{PullRequestAccountKind, ReviewTeam, Team};
use super::repository::WorkflowOwner;
use super::repository::pull::PullRepository;
use super::{Installation, Organization};
use crate::check::EnterpriseRecord;
use crate::owner::OwnerRecord;
use crate::pull::PullRefRecord;
use crate::repository::pull::PullRepositoryRecord;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
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
            PullRefRecord<Option<WorkflowOwner>, PullRepository>,
            Team,
            String,
        >,
    },
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    deny_unknown_fields,
    bound(
        deserialize = "OriginalLine: Deserialize<'de>, Account: Deserialize<'de>, Head: Deserialize<'de>, RequestedTeam: Deserialize<'de>, Title: Deserialize<'de>"
    )
)]
pub struct ThreadPayload<
    OriginalLine = Nullable<UInt>,
    Account = WorkflowOwner<PullRequestAccountKind>,
    Head = PullRefRecord<Option<WorkflowOwner>, Option<PullRepository>, Nullable<String>>,
    RequestedTeam = ReviewTeam,
    Title = Option<String>,
> {
    pub thread: ReviewThread<ReviewCommentRecord<OriginalLine, Account>>,
    pub pull_request: ThreadPullRequest<Account, Head, RequestedTeam, Title>,
    pub repository: PullRepositoryRecord,
    pub sender: Option<OwnerRecord>,
    pub installation: Option<Installation>,
    pub organization: Option<Organization>,
    pub enterprise: Option<EnterpriseRecord>,
    pub updated_at: Option<Nullable<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewThread<
    Comment = ReviewCommentRecord<Nullable<UInt>, WorkflowOwner<PullRequestAccountKind>>,
> {
    pub node_id: String,
    pub comments: Vec<Comment>,
}
