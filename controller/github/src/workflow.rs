use amiss_wire::model::Oid;
use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::pull::PullRefRecord;
use crate::repository::WorkflowRepositoryRecord;

#[serde_with::apply(
    u64 => #[serde(with = "As::<TryFromInto<UInt>>")],
    Option<_> => #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")],
)]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowRunRecord {
    pub id: u64,
    pub head_sha: Oid,
    pub event: String,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub status: Option<String>,
    #[serde_with(skip_apply)]
    #[serde(deserialize_with = "Option::deserialize")]
    pub conclusion: Option<String>,
    pub workflow_id: u64,
    pub head_repository: WorkflowRepositoryRecord,
    pub repository: WorkflowRepositoryRecord,
    pub run_attempt: Option<UInt>,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowRunPage {
    pub total_count: u64,
    pub workflow_runs: Vec<WorkflowRunRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(bound(deserialize = "User: Deserialize<'de>"))]
pub struct WorkflowCommit<User = Option<WorkflowCommitUser>> {
    pub id: Oid,
    pub tree_id: Oid,
    pub message: String,
    pub timestamp: String,
    #[serde(deserialize_with = "User::deserialize")]
    pub author: User,
    #[serde(deserialize_with = "User::deserialize")]
    pub committer: User,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowCommitUser {
    pub name: String,
    pub email: String,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowPullRequest {
    pub id: u64,
    pub number: u64,
    pub head: PullRefRecord<WorkflowPullRepository>,
    pub base: PullRefRecord<WorkflowPullRepository>,
}

#[serde_with::apply(u64 => #[serde(with = "As::<TryFromInto<UInt>>")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowPullRepository {
    pub id: u64,
    pub name: String,
}
