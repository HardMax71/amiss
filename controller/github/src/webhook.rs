use amiss_wire::model::Oid;
use js_int::UInt;
use serde::{Deserialize, Serialize};
use serde_with::{As, TryFromInto};

use crate::workflow::WorkflowPullRequest;

#[serde_with::apply(Option<_> => #[serde(skip_serializing_if = "Option::is_none")])]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct GitHubPayload {
    pub action: Option<String>,
    pub changes: Option<PullRequestChanges>,
    pub installation: Option<Installation>,
    pub repository: Option<Repository>,
    pub number: Option<u64>,
    pub pull_request: Option<PullRequest>,
    pub workflow: Option<Workflow>,
    pub workflow_run: Option<WorkflowRun>,
}

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
#[serde(deny_unknown_fields)]
pub struct Installation {
    #[serde(with = "As::<TryFromInto<UInt>>")]
    pub id: u64,
    pub node_id: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub event: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub workflow_id: u64,
    pub run_attempt: u64,
    pub head_sha: Oid,
    pub repository: Repository,
    pub head_repository: Repository,
    pub pull_requests: Vec<WorkflowPullRequest>,
}
