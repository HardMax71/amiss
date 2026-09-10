use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use super::app::WebhookApp;
use super::{Committer, Installation, Organization, WorkflowRunConclusion};
use crate::check::{CheckRunStatus, EnterpriseRecord};
use crate::owner::OwnerRecord;
use crate::repository::pull::PullRepositoryRecord;
use crate::workflow::{WorkflowCommit, WorkflowPullRequest};

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSuiteEvent {
    pub action: CheckSuiteAction,
    pub check_suite: CheckSuiteRecord,
    pub repository: PullRepositoryRecord,
    pub sender: OwnerRecord,
    pub installation: Option<Installation>,
    pub organization: Option<Organization>,
    pub enterprise: Option<EnterpriseRecord>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum CheckSuiteAction {
    Completed,
    Requested,
    Rerequested,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSuiteRecord {
    pub id: UInt,
    pub node_id: String,
    pub head_branch: Nullable<String>,
    pub head_sha: Oid,
    pub status: Nullable<CheckRunStatus>,
    pub conclusion: Nullable<WorkflowRunConclusion>,
    pub url: String,
    pub before: Nullable<Oid>,
    pub after: Nullable<Oid>,
    pub pull_requests: Vec<WorkflowPullRequest>,
    pub app: WebhookApp,
    pub created_at: String,
    pub updated_at: String,
    pub latest_check_runs_count: UInt,
    pub check_runs_url: String,
    pub head_commit: WorkflowCommit<Committer>,
    pub rerequestable: Option<bool>,
    pub runs_rerequestable: Option<bool>,
}
