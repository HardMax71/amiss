use amiss_wire::assessment::Nullable;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

use super::review::PullRequestContext;
use super::{PullRequestAccountKind, ReviewTeam, Reviewer};
use crate::owner::OwnerRecord;
use crate::pull::PullRefRecord;
use crate::pull::metadata::{AutoMergeRecord, StackRecord};
use crate::repository::WorkflowRepositoryRecord;
use crate::webhook::repository::WorkflowOwner;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(
    deserialize = "Account: Deserialize<'de>, Head: Deserialize<'de>, RequestedTeam: Deserialize<'de>, Title: Deserialize<'de>"
))]
pub struct ThreadPullRequest<
    Account = WorkflowOwner<PullRequestAccountKind>,
    Head = PullRefRecord<Option<WorkflowRepositoryRecord<Option<OwnerRecord>>>>,
    RequestedTeam = ReviewTeam,
    Title = Option<String>,
> {
    #[serde(flatten)]
    pub context: PullRequestContext<
        WorkflowOwner,
        Reviewer<Account, RequestedTeam>,
        WorkflowOwner,
        Account,
        Head,
    >,
    pub auto_merge: Nullable<AutoMergeRecord<Option<WorkflowOwner>, Option<String>, Title>>,
    pub draft: bool,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub stack: Option<StackRecord>,
}
