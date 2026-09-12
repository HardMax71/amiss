use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::app::WebhookApp;
use super::{Absent, Installation, WorkflowRunConclusion};
use crate::check::CheckRunRecord;
use crate::repository::WorkflowRepositoryRecord;

#[serde_with::apply(
    Option<_> => #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )],
    Absent => #[serde(default, skip_serializing)]
)]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CheckRunEvent<Action = CheckRunActivity> {
    #[serde(flatten)]
    pub action: Action,
    pub check_run: CheckRunRecord<WebhookApp, WebhookCheckRunConclusion>,
    pub repository: WorkflowRepositoryRecord,
    pub installation: Option<Installation>,
    pub number: Absent,
    pub pull_request: Absent,
    pub issue: Absent,
    pub review: Absent,
    pub comment: Absent,
    pub thread: Absent,
    pub check_suite: Absent,
    pub workflow: Absent,
    pub workflow_run: Absent,
    pub changes: Absent,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CheckRunActivity {
    pub action: Option<CheckRunAction>,
    #[serde(default, skip_serializing)]
    pub requested_action: Absent,
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
