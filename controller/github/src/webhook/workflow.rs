use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::{Absent, Installation, Workflow, WorkflowRun};
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
pub struct WorkflowRunEvent<Action = WorkflowRunAction> {
    pub action: Action,
    pub repository: WorkflowRepositoryRecord,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub workflow: Option<Workflow>,
    pub workflow_run: WorkflowRun,
    pub installation: Option<Installation>,
    pub number: Absent,
    pub pull_request: Absent,
    pub issue: Absent,
    pub review: Absent,
    pub comment: Absent,
    pub thread: Absent,
    pub check_suite: Absent,
    pub check_run: Absent,
    pub requested_action: Absent,
    pub changes: Absent,
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
