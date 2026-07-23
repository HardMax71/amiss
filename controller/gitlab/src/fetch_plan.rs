use std::fmt;

use amiss_controller::{RunIdentity, RunRequest};
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid};

use crate::identity::{
    canonical_repository, exact_sha1, parse_change_id, parse_delivery_id, parse_run_id,
    repository_url,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitLabPlanError;

impl fmt::Display for GitLabPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the GitLab acquisition request is inconsistent")
    }
}

impl std::error::Error for GitLabPlanError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitLabFetchPlan {
    pub project_id: u64,
    pub pipeline_id: u64,
    pub job_id: u64,
    pub repository_url: String,
    pub repository_oids: [Oid; 2],
    pub action_url: String,
    pub action_oid: Oid,
}

/// Projects an authenticated GitLab run into credential-free exact fetch inputs.
///
/// The URLs and object IDs map directly onto `amiss-controller-git`'s exact
/// fetch values.
///
/// # Errors
///
/// The request does not reproduce its GitLab policy-job identity and SHA-1
/// object bindings.
pub fn gitlab_fetch_plan(request: &RunRequest) -> Result<GitLabFetchPlan, GitLabPlanError> {
    let run = &request.run;
    let provider = &request.delivery.provider;
    let repository = &run.change.repository;
    let action = &request.plan.execution.action_repository;
    let (project_id, _merge_request_iid) =
        parse_change_id(run.change.change.as_str()).ok_or(GitLabPlanError)?;
    let (pipeline_id, job_id) =
        parse_run_id(request.provider_run.run_id.as_str()).ok_or(GitLabPlanError)?;
    let _runner_id =
        parse_delivery_id(request.delivery.delivery.as_str()).ok_or(GitLabPlanError)?;
    let identity_valid = provider.namespace.as_str() == "gitlab"
        && request.delivery.provider == run.change.provider
        && repository.host == provider.instance.as_str()
        && action.host == provider.instance.as_str()
        && canonical_repository(repository)
        && canonical_repository(action);
    let format_valid = run.refs.forge == ForgeDialect::Gitlab
        && run.object_format == ObjectFormat::Sha1
        && request.provider_run.object_format == ObjectFormat::Sha1
        && request.plan.execution.action_object_format == ObjectFormat::Sha1;
    let binding_valid = request.provider_run.attempt.get() == 1
        && request.provider_run.candidate_commit == run.commits.candidate
        && exact_oids(run, request);
    let refs_valid = [
        run.refs.candidate.as_str(),
        run.refs.target.as_str(),
        run.refs.default_branch.as_str(),
    ]
    .into_iter()
    .all(|reference| reference.starts_with("refs/heads/"));
    if !identity_valid || !format_valid || !binding_valid || !refs_valid {
        return Err(GitLabPlanError);
    }
    let project_path = format!("{}/{}", repository.owner, repository.name);
    let action_path = format!("{}/{}", action.owner, action.name);
    Ok(GitLabFetchPlan {
        project_id,
        pipeline_id,
        job_id,
        repository_url: repository_url(&repository.host, &project_path).ok_or(GitLabPlanError)?,
        repository_oids: [run.commits.base.clone(), run.commits.candidate.clone()],
        action_url: repository_url(&action.host, &action_path).ok_or(GitLabPlanError)?,
        action_oid: request.plan.execution.action_commit_oid.clone(),
    })
}

fn exact_oids(run: &RunIdentity, request: &RunRequest) -> bool {
    [
        &run.commits.base,
        &run.commits.candidate,
        &run.trees.base,
        &run.trees.candidate,
        &request.plan.execution.action_commit_oid,
        &request.plan.execution.action_tree_oid,
    ]
    .into_iter()
    .all(|oid| exact_sha1(oid.as_str()).as_ref() == Some(oid))
}
