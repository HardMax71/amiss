use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, DeserializeFromStr, SerializeDisplay, TryFromInto};
use strum::{Display, EnumString};

use crate::check::CheckRunStatus;
use crate::repository::pull::PullRepositoryRecord;
use crate::workflow::{ReferencedWorkflow, WorkflowCommit, WorkflowPullRequest};

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

use pull::request::PullRequestWebhook;
use repository::{WorkflowOwner, WorkflowRepository};

#[serde_with::apply(Option<_> => #[serde(skip_serializing_if = "Option::is_none")])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "Pull: Deserialize<'de>, Action: Deserialize<'de>"))]
pub struct GitHubPayload<Pull = PullRequestWebhook, Action = String> {
    pub action: Option<Action>,
    pub changes: Option<PullRequestChanges>,
    pub installation: Option<Installation>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub repository: Option<PullRepositoryRecord>,
    pub number: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub issue: Option<issue::IssueRecord<issue::context::IssueActivityContext>>,
    pub pull_request: Option<Pull>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub review: Option<review::ReviewRecord>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub comment: Option<comment::Comment>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub thread: Option<thread::ReviewThread>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub check_suite: Option<suite::CheckSuiteRecord>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub check_run: Option<
        crate::check::CheckRunRecord<
            app::WebhookApp<Nullable<crate::check::AppOwner>>,
            run::WebhookCheckRunConclusion,
        >,
    >,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub workflow: Option<Nullable<Workflow>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub workflow_run: Option<WorkflowRun>,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestChanges {
    pub base: Option<BaseChange>,
    pub body: Option<PreviousReference>,
    pub title: Option<PreviousReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaseChange {
    #[serde(rename = "ref")]
    pub reference: PreviousReference,
    pub sha: PreviousReference<Oid>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviousReference<T = String> {
    pub from: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Installation {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub node_id: String,
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
#[serde(deny_unknown_fields)]
pub struct Workflow {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub node_id: String,
    pub name: String,
    pub path: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub url: String,
    pub html_url: String,
    pub badge_url: String,
}

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    Option<_> => #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")],
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRun<Title = workflow::WorkflowRunTitle> {
    pub id: u64,
    pub event: String,
    pub status: CheckRunStatus,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub conclusion: Option<WorkflowRunConclusion>,
    pub workflow_id: u64,
    pub run_attempt: u64,
    pub head_sha: Oid,
    pub head_commit: WorkflowCommit<Committer>,
    pub repository: WorkflowRepository,
    pub head_repository: WorkflowRepository,
    pub pull_requests: Vec<Option<WorkflowPullRequest>>,
    pub actor: Nullable<WorkflowOwner>,
    pub artifacts_url: String,
    pub cancel_url: String,
    pub check_suite_id: UInt,
    pub check_suite_node_id: String,
    pub check_suite_url: String,
    pub created_at: String,
    pub head_branch: Nullable<String>,
    pub html_url: String,
    pub jobs_url: String,
    pub logs_url: String,
    pub name: Nullable<String>,
    pub node_id: String,
    pub path: String,
    pub previous_attempt_url: Nullable<String>,
    pub rerun_url: String,
    pub run_number: UInt,
    pub run_started_at: String,
    pub triggering_actor: Nullable<WorkflowOwner>,
    pub updated_at: String,
    pub url: String,
    pub workflow_url: String,
    #[serde(flatten)]
    pub title: Title,
    pub referenced_workflows: Option<Nullable<Vec<ReferencedWorkflow>>>,
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
