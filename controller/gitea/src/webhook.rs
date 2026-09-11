use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::pull::PullRequestRecord;
use crate::repository::RepositoryRecord;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum HookIssueAction {
    Opened,
    Closed,
    Reopened,
    Edited,
    Deleted,
    Assigned,
    Unassigned,
    LabelUpdated,
    LabelCleared,
    Synchronized,
    Milestoned,
    Demilestoned,
    Reviewed,
    ReviewRequested,
    ReviewRequestRemoved,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRequestPayload {
    pub action: HookIssueAction,
    pub repository: RepositoryRecord,
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub number: u64,
    pub pull_request: PullRequestRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<PullRequestChanges>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRequestChanges {
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<PreviousReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PreviousReference {
    pub from: String,
}
