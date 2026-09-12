use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::check::CheckRunStatus;
use crate::owner::OwnerRecord;
use crate::repository::WorkflowRepositoryRecord;
use crate::workflow::WorkflowPullRequest;

pub use json_serde::Absent;

pub mod app;
pub mod comment;
pub mod event;
pub mod issue;
pub mod pull;
pub mod repository;
pub mod review;
pub mod run;
pub mod suite;
pub mod thread;
pub mod workflow;

use crate::pull::PullRequestRecord;

#[serde_with::apply(
    Option<_> => #[serde(skip_serializing_if = "Option::is_none")],
    Absent => #[serde(default, skip_serializing)]
)]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "Pull: Deserialize<'de>, Action: Deserialize<'de>"))]
pub struct GitHubPayload<Pull = PullRequestRecord, Action = String> {
    pub action: Option<Action>,
    pub changes: Option<PullRequestChanges>,
    pub installation: Option<Installation>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub repository: Option<WorkflowRepositoryRecord>,
    pub number: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub issue: Option<issue::IssueRecord<issue::context::IssueActivityContext>>,
    pub pull_request: Option<Pull>,
    pub review: Absent,
    pub comment: Absent,
    pub thread: Absent,
    pub check_suite: Absent,
    pub check_run: Absent,
    pub workflow: Absent,
    pub workflow_run: Absent,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRequestChanges {
    pub base: Option<BaseChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct BaseChange {
    #[serde(rename = "ref")]
    pub reference: PreviousReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PreviousReference {
    pub from: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Installation {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Organization {
    pub login: String,
    pub id: UInt,
    pub node_id: String,
    pub url: String,
    pub repos_url: String,
    pub events_url: String,
    pub hooks_url: String,
    pub issues_url: String,
    pub members_url: String,
    pub public_members_url: String,
    pub avatar_url: String,
    pub description: Nullable<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Repository {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner: Owner,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Owner {
    pub login: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PullRequest {
    pub id: u64,
    pub number: u64,
    pub head: Head,
    pub base: Base,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Head {
    pub sha: Oid,
    #[serde(rename = "ref")]
    pub branch: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Base {
    #[serde(rename = "ref")]
    pub branch: String,
    pub repo: Repository,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Workflow {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub path: String,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub event: String,
    pub status: CheckRunStatus,
    #[serde(deserialize_with = "Option::deserialize")]
    pub conclusion: Option<WorkflowRunConclusion>,
    pub workflow_id: u64,
    pub run_attempt: u64,
    pub head_sha: Oid,
    pub repository: WorkflowRepositoryRecord<Option<OwnerRecord>>,
    pub head_repository: WorkflowRepositoryRecord<Option<OwnerRecord>>,
    pub pull_requests: Vec<Option<WorkflowPullRequest>>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum WorkflowRunConclusion {
    ActionRequired,
    Cancelled,
    Failure,
    Neutral,
    Skipped,
    Stale,
    Success,
    TimedOut,
    StartupFailure,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Committer {
    pub name: String,
    pub email: Nullable<String>,
    pub date: Option<String>,
    pub username: Option<String>,
}
