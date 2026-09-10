use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::app::WebhookApp;
use super::{Installation, Organization, WorkflowRunConclusion};
use crate::check::{
    AppOwner, CheckRunDeployment, CheckRunRecord, CheckRunStatus, EnterpriseRecord,
};
use crate::owner::OwnerRecord;
use crate::repository::WorkflowRepositoryRecord;
use crate::repository::pull::PullRepositoryRecord;
use crate::workflow::WorkflowPullRequest;

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRunEvent<Action = CheckRunActivity> {
    #[serde(flatten)]
    pub action: Action,
    pub check_run: CheckRunRecord<
        WebhookCheckRunResource,
        WebhookCheckSuite,
        WebhookApp<Nullable<AppOwner>>,
        WebhookCheckRunConclusion,
    >,
    pub repository: PullRepositoryRecord<WebhookRepositoryAvailability>,
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
pub struct CheckRunActivity {
    pub action: Option<CheckRunAction>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum CheckRunAction {
    Created,
    Completed,
    Rerequested,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedAction {
    pub action: RequestedActionKind,
    pub requested_action: Option<RequestedActionIdentifier>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum RequestedActionKind {
    RequestedAction,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedActionIdentifier {
    pub identifier: Option<String>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookCheckRunResource {
    pub node_id: Option<String>,
    pub details_url: Option<Nullable<String>>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookRepositoryAvailability {
    pub disabled: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum WebhookCheckRunConclusion {
    Completed(WorkflowRunConclusion),
    Pending(PendingCheckRunConclusion),
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum PendingCheckRunConclusion {
    Waiting,
    Pending,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookCheckSuite {
    pub id: Option<UInt>,
    pub node_id: Option<String>,
    pub head_branch: Option<Nullable<String>>,
    pub head_sha: Option<Oid>,
    pub status: Option<CheckRunStatus>,
    pub conclusion: Option<Nullable<WorkflowRunConclusion>>,
    pub url: Option<String>,
    pub before: Option<Nullable<Oid>>,
    pub after: Option<Nullable<Oid>>,
    pub pull_requests: Option<Vec<WorkflowPullRequest>>,
    pub app: Option<Nullable<WebhookApp<Nullable<AppOwner>>>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub repository: Option<WorkflowRepositoryRecord>,
    pub deployment: Option<CheckRunDeployment<WebhookApp<Nullable<AppOwner>>>>,
}
