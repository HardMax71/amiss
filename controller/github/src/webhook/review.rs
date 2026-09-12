use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::pull::PullRequestAccountKind;
use super::pull::review::{ReviewActivityPullRequest, ReviewPullRequest};
use super::repository::WorkflowOwner;
use super::{Installation, Organization, PreviousReference};
use crate::check::EnterpriseRecord;
use crate::owner::OwnerRecord;
use crate::pull::metadata::{AuthorAssociation, Link};
use crate::repository::pull::PullRepositoryRecord;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewEvent {
    Submitted {
        #[serde(flatten)]
        event: ReviewPayload,
    },
    Dismissed {
        #[serde(flatten)]
        event: ReviewPayload<
            ReviewActivityPullRequest,
            ReviewRecord<DismissedReviewState, String, WorkflowOwner<PullRequestAccountKind>>,
        >,
    },
    Edited {
        changes: ReviewChanges,
        #[serde(flatten)]
        event: ReviewPayload<ReviewPullRequest>,
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
    bound(deserialize = "Pull: Deserialize<'de>, Review: Deserialize<'de>")
)]
pub struct ReviewPayload<Pull = ReviewActivityPullRequest, Review = ReviewRecord> {
    pub review: Review,
    pub pull_request: Pull,
    pub repository: PullRepositoryRecord,
    pub sender: OwnerRecord,
    pub installation: Option<Installation>,
    pub organization: Option<Organization>,
    pub enterprise: Option<EnterpriseRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    deny_unknown_fields,
    bound(
        deserialize = "State: Deserialize<'de>, SubmittedAt: Deserialize<'de>, User: Deserialize<'de>"
    )
)]
pub struct ReviewRecord<State = String, SubmittedAt = Nullable<String>, User = WorkflowOwner> {
    pub id: UInt,
    pub node_id: String,
    pub user: Nullable<User>,
    pub body: Nullable<String>,
    pub commit_id: Oid,
    #[serde(deserialize_with = "SubmittedAt::deserialize")]
    pub submitted_at: SubmittedAt,
    pub state: State,
    pub html_url: String,
    pub pull_request_url: String,
    pub author_association: AuthorAssociation,
    #[serde(rename = "_links")]
    pub links: ReviewLinks,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<Nullable<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewLinks {
    pub html: Link,
    pub pull_request: Link,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewChanges {
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub body: Option<PreviousReference>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum DismissedReviewState {
    Dismissed,
    Approved,
    ChangesRequested,
}
