use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

use super::Reactions;
use crate::check::{CheckRunApp, EnterpriseRecord};
use crate::owner::OwnerRecord;
use crate::pull::metadata::AuthorAssociation;
use crate::repository::pull::PullRepositoryRecord;
use crate::webhook::issue::IssueRecord;
use crate::webhook::pull::PullRequestAccountKind;
use crate::webhook::repository::WorkflowOwner;
use crate::webhook::review::ReviewChanges;
use crate::webhook::{Installation, Organization};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum IssueCommentEvent {
    Created {
        #[serde(flatten)]
        event: IssueCommentPayload<IssueCommentRecord<WorkflowOwner>>,
    },
    Edited {
        changes: ReviewChanges,
        #[serde(flatten)]
        event: IssueCommentPayload,
    },
    Deleted {
        #[serde(flatten)]
        event: IssueCommentPayload,
    },
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCommentPayload<Comment = IssueCommentRecord> {
    pub issue: IssueRecord,
    pub comment: Comment,
    pub repository: PullRepositoryRecord,
    pub sender: OwnerRecord,
    pub installation: Option<Installation>,
    pub organization: Option<Organization>,
    pub enterprise: Option<EnterpriseRecord>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCommentRecord<
    User = WorkflowOwner<PullRequestAccountKind>,
    Metadata = IssueCommentMetadata,
> {
    pub url: String,
    pub html_url: String,
    pub issue_url: String,
    pub id: UInt,
    pub node_id: String,
    pub user: Nullable<User>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(flatten)]
    pub metadata: Metadata,
    pub minimized: Option<Nullable<MinimizedComment>>,
    pub pin: Option<Nullable<PinnedComment>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IssueCommentMetadata {
    pub author_association: AuthorAssociation,
    pub performed_via_github_app: Nullable<Box<CheckRunApp>>,
    pub body: String,
    pub reactions: Reactions,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedIssueCommentMetadata {
    pub author_association: Option<AuthorAssociation>,
    pub performed_via_github_app: Option<Nullable<Box<CheckRunApp>>>,
    pub body: Option<String>,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub reactions: Option<Reactions>,
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
