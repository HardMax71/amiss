use amiss_wire::model::Oid;
use serde::Serialize;

use crate::check::{CheckRunConclusion, CheckRunStatus};
pub(super) use crate::check::{CheckRunPage, CheckRunRecord};
pub(super) use crate::commit::GitCommitRecord;
pub(super) use crate::pull::PullRequestRecord;
pub(super) use crate::reference::RefRecord;
pub(super) use crate::repository::RepositoryRecord;
pub(super) use crate::repository::WorkflowRepositoryRecord;

use super::rules::BranchRule;

#[derive(Clone)]
pub(super) struct CommitRecord {
    pub sha: Oid,
    pub tree: Oid,
}

#[derive(Clone)]
pub(super) struct RefreshData {
    pub repository: RepositoryRecord,
    pub pull_request: PullRequestRecord,
    pub target: CommitRecord,
    pub candidate: CommitRecord,
    pub current_head: CommitRecord,
    pub gate: GateCommitRecord,
    pub rules: Vec<BranchRule>,
}

#[derive(Clone)]
pub(super) struct GateCommitRecord {
    pub sha: Oid,
    pub tree: Oid,
    pub parents: Vec<Oid>,
}

#[derive(Clone, Serialize)]
pub(super) struct CreateCheckRun {
    pub name: String,
    pub head_sha: Oid,
    pub external_id: String,
    pub status: CheckRunStatus,
    pub conclusion: CheckRunConclusion,
    pub output: CreateCheckRunOutput,
}

#[derive(Clone, Serialize)]
pub(super) struct CreateCheckRunOutput {
    pub title: String,
    pub summary: String,
}
