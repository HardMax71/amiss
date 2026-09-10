use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::{Installation, Organization, Workflow, WorkflowRun};
use crate::check::EnterpriseRecord;
use crate::owner::OwnerRecord;
use crate::repository::pull::PullRepositoryRecord;

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRunEvent<Run = WorkflowRun, Action = WorkflowRunAction> {
    pub action: Action,
    pub repository: PullRepositoryRecord,
    pub sender: OwnerRecord,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub workflow: Option<Workflow>,
    pub workflow_run: Run,
    pub installation: Option<Installation>,
    pub organization: Option<Organization>,
    pub enterprise: Option<EnterpriseRecord>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum WorkflowRunAction {
    Completed,
    InProgress,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum RequestedWorkflowRunAction {
    Requested,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRunTitle {
    pub display_title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestedWorkflowRunTitle {
    pub display_title: String,
}
