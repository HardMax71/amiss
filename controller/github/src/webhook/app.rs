use amiss_wire::assessment::Nullable;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::repository::WorkflowOwner;
use crate::installation::permissions::{AppPermissions, ReadWrite, WriteOnly};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "Owner: Deserialize<'de>"))]
pub struct WebhookApp<Owner = Nullable<WorkflowOwner>> {
    pub id: Nullable<UInt>,
    pub node_id: String,
    #[serde(deserialize_with = "Owner::deserialize")]
    pub owner: Owner,
    pub name: String,
    pub description: Nullable<String>,
    pub external_url: Nullable<String>,
    pub html_url: String,
    pub created_at: Nullable<String>,
    pub updated_at: Nullable<String>,
    pub permissions: Option<WebhookAppPermissions>,
    pub events: Option<Vec<AppEvent>>,
    pub slug: Option<String>,
    pub client_id: Option<Nullable<String>>,
    pub installations_count: Option<UInt>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookAppPermissions {
    #[serde(flatten)]
    pub permissions: AppPermissions<ReadWrite, ReadWrite>,
    pub content_references: Option<ReadWrite>,
    pub copilot_requests: Option<WriteOnly>,
    pub drives: Option<ReadWrite>,
    pub emails: Option<ReadWrite>,
    pub keys: Option<ReadWrite>,
    pub models: Option<ReadWrite>,
    pub security_scanning_alert: Option<ReadWrite>,
    pub team_discussions: Option<ReadWrite>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum AppEvent {
    BranchProtectionRule,
    CheckRun,
    CheckSuite,
    CodeScanningAlert,
    CommitComment,
    ContentReference,
    Create,
    Delete,
    Deployment,
    DeploymentReview,
    DeploymentStatus,
    DeployKey,
    Discussion,
    DiscussionComment,
    Fork,
    Gollum,
    Issues,
    IssueComment,
    Label,
    Member,
    Membership,
    Milestone,
    Organization,
    OrgBlock,
    PageBuild,
    Project,
    ProjectCard,
    ProjectColumn,
    Public,
    PullRequest,
    PullRequestReview,
    PullRequestReviewComment,
    Push,
    RegistryPackage,
    Release,
    Repository,
    RepositoryDispatch,
    SecretScanningAlert,
    Star,
    Status,
    Team,
    TeamAdd,
    Watch,
    WorkflowDispatch,
    WorkflowRun,
    MergeGroup,
    PullRequestReviewThread,
    WorkflowJob,
    MergeQueueEntry,
    SecurityAndAnalysis,
    ProjectsV2Item,
    SecretScanningAlertLocation,
    RepositoryImport,
}
