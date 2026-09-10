use amiss_wire::assessment::Nullable;
use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::owner::OwnerRecord;
use crate::repository::WorkflowRepositoryRecord;

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    Option<_> => #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")],
)]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRunRecord {
    pub id: u64,
    pub node_id: String,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub head_branch: Option<String>,
    pub head_sha: Oid,
    pub run_number: u64,
    pub display_title: String,
    pub event: String,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub status: Option<String>,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub conclusion: Option<String>,
    pub path: String,
    pub workflow_id: u64,
    pub url: String,
    pub html_url: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub head_commit: Option<WorkflowCommit>,
    pub head_repository: WorkflowRepositoryRecord,
    pub repository: WorkflowRepositoryRecord,
    pub jobs_url: String,
    pub logs_url: String,
    pub check_suite_url: String,
    pub cancel_url: String,
    pub rerun_url: String,
    pub artifacts_url: String,
    pub workflow_url: String,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub pull_requests: Option<Vec<WorkflowPullRequest>>,
    pub actor: Option<OwnerRecord>,
    pub check_suite_id: Option<UInt>,
    pub check_suite_node_id: Option<String>,
    pub head_repository_id: Option<UInt>,
    pub name: Option<Nullable<String>>,
    pub previous_attempt_url: Option<Nullable<String>>,
    pub referenced_workflows: Option<Nullable<Vec<ReferencedWorkflow>>>,
    pub run_attempt: Option<UInt>,
    pub run_started_at: Option<String>,
    pub triggering_actor: Option<OwnerRecord>,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRunPage {
    pub total_count: u64,
    pub workflow_runs: Vec<WorkflowRunRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowCommit {
    pub id: Oid,
    pub tree_id: Oid,
    pub message: String,
    pub timestamp: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub author: Option<WorkflowCommitUser>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub committer: Option<WorkflowCommitUser>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowCommitUser {
    pub name: String,
    pub email: String,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPullRequest {
    pub id: u64,
    pub number: u64,
    pub url: String,
    pub head: WorkflowPullRef,
    pub base: WorkflowPullRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPullRef {
    #[serde(rename = "ref")]
    pub branch: String,
    pub sha: Oid,
    pub repo: WorkflowPullRepository,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPullRepository {
    pub id: u64,
    pub name: String,
    pub url: String,
}

#[serde_with::apply(Option<_> => #[serde(
    default,
    deserialize_with = "deserialize_some",
    skip_serializing_if = "Option::is_none"
)])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencedWorkflow {
    pub path: String,
    pub sha: Oid,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
}
