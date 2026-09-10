use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

use super::Reactions;
use crate::check::CheckRunApp;
use crate::owner::OwnerRecord;
use crate::pull::metadata::AuthorAssociation;
use crate::webhook::pull::PullRequestAccountKind;
use crate::webhook::repository::WorkflowOwner;

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCommentRecord {
    pub url: String,
    pub html_url: String,
    pub issue_url: String,
    pub id: UInt,
    pub node_id: String,
    pub user: Nullable<WorkflowOwner<PullRequestAccountKind>>,
    pub created_at: String,
    pub updated_at: String,
    pub author_association: AuthorAssociation,
    pub performed_via_github_app: Nullable<Box<CheckRunApp>>,
    pub body: String,
    pub reactions: Reactions,
    pub minimized: Option<Nullable<MinimizedComment>>,
    pub pin: Option<Nullable<PinnedComment>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MinimizedComment {
    pub reason: Nullable<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedComment {
    pub pinned_at: String,
    pub pinned_by: Nullable<OwnerRecord>,
}
