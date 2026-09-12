use amiss_wire::assessment::Nullable;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::app::WebhookApp;
use super::{Installation, Organization, WorkflowRunConclusion};
use crate::check::{AppOwner, CheckRunRecord, EnterpriseRecord};
use crate::owner::OwnerRecord;
use crate::repository::WorkflowRepositoryRecord;

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
    pub check_run: CheckRunRecord<WebhookApp<Nullable<AppOwner>>, WebhookCheckRunConclusion>,
    pub repository: WorkflowRepositoryRecord,
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
