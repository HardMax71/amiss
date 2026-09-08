use amiss_wire::model::Oid;
use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, NoneAsEmptyString, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::issue::Label;
use crate::pull::PullRequestRecord;
use crate::repository::RepositoryRecord;
use crate::user::UserRecord;

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

#[serde_with::apply(
    Option<RepositoryRecord> => #[serde(deserialize_with = "Option::deserialize")],
    Option<PullRequestRecord> => #[serde(deserialize_with = "Option::deserialize")],
    Option<UserRecord> => #[serde(deserialize_with = "Option::deserialize")],
    Option<ReviewPayload> => #[serde(deserialize_with = "Option::deserialize")],
    Option<PullRequestChanges> => #[serde(default, deserialize_with = "json_serde::deserialize_some", skip_serializing_if = "Option::is_none")],
    Option<Oid> => #[serde(default, deserialize_with = "json_serde::deserialize_some", skip_serializing_if = "Option::is_none")],
    Option<Label> => #[serde(default, deserialize_with = "json_serde::deserialize_some", skip_serializing_if = "Option::is_none")]
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestPayload {
    pub action: HookIssueAction,
    pub repository: Option<RepositoryRecord>,
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub number: u64,
    pub pull_request: Option<PullRequestRecord>,
    pub requested_reviewer: Option<UserRecord>,
    pub sender: Option<UserRecord>,
    #[serde_with(skip_apply)]
    #[serde(with = "As::<NoneAsEmptyString>")]
    pub commit_id: Option<Oid>,
    pub review: Option<ReviewPayload>,
    pub changes: Option<PullRequestChanges>,
    pub before: Option<Oid>,
    pub after: Option<Oid>,
    pub label: Option<Label>,
}

#[serde_with::apply(
    Option<PreviousReference> => #[serde(default, deserialize_with = "json_serde::deserialize_some", skip_serializing_if = "Option::is_none")],
    Option<Vec<Label>> => #[serde(deserialize_with = "Option::deserialize")]
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum PullRequestChanges {
    Gitea {
        title: Option<PreviousReference>,
        body: Option<PreviousReference>,
        #[serde(rename = "ref")]
        reference: Option<PreviousReference>,
        name: Option<PreviousReference>,
        added_labels: Option<Vec<Label>>,
        removed_labels: Option<Vec<Label>>,
    },
    Forgejo {
        title: Option<PreviousReference>,
        body: Option<PreviousReference>,
        #[serde(rename = "ref")]
        reference: Option<PreviousReference>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviousReference {
    pub from: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPayload {
    #[serde(rename = "type")]
    pub kind: ReviewType,
    pub content: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ReviewType {
    PullRequestReviewApproved,
    PullRequestReviewRejected,
    PullRequestReviewComment,
}
