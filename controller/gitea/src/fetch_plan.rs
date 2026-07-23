use std::fmt;

use amiss_controller::{RunIdentity, RunRequest};
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use crate::identity::{canonical_host, parse_change_id, positive, provider_run};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiteaPlanError {
    InvalidRequest,
}

impl fmt::Display for GiteaPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the Gitea-family acquisition request is inconsistent")
    }
}

impl std::error::Error for GiteaPlanError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GiteaFetchPlan {
    pub integration_id: u64,
    pub repository_url: String,
    pub repository_oids: [Oid; 2],
    pub action_url: String,
    pub action_oid: Oid,
}

/// Projects a validated provider request into credential-free exact fetch inputs.
///
/// The returned URLs, object IDs, and destinations map directly onto
/// `amiss-controller-git`'s `ExactFetch` and `ExactWant` values.
///
/// # Errors
///
/// The request does not reproduce the authenticated Gitea-family identity.
pub fn gitea_fetch_plan(request: &RunRequest) -> Result<GiteaFetchPlan, GiteaPlanError> {
    let run = &request.run;
    let provider = &request.delivery.provider;
    let repository = &run.change.repository;
    let action = &request.plan.execution.action_repository;
    let integration_id = request
        .delivery
        .integration
        .as_str()
        .parse::<u64>()
        .ok()
        .and_then(positive)
        .ok_or(GiteaPlanError::InvalidRequest)?;
    let _change =
        parse_change_id(run.change.change.as_str()).ok_or(GiteaPlanError::InvalidRequest)?;
    let expected_run = provider_run(
        &request.delivery.integration,
        &run.change,
        &run.commits.candidate,
        &run.refs.candidate,
        &run.refs.target,
    )
    .ok_or(GiteaPlanError::InvalidRequest)?;

    let identity_valid = request.delivery.provider == run.change.provider
        && repository.host == provider.instance.as_str()
        && action.host == provider.instance.as_str()
        && canonical_repository(repository)
        && canonical_repository(action);
    let format_valid = run.refs.forge == ForgeDialect::Gitea
        && run.object_format == ObjectFormat::Sha1
        && request.provider_run.object_format == ObjectFormat::Sha1
        && request.plan.execution.action_object_format == ObjectFormat::Sha1;
    let binding_valid = request.provider_run == expected_run
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
        return Err(GiteaPlanError::InvalidRequest);
    }

    Ok(GiteaFetchPlan {
        integration_id,
        repository_url: repository_url(repository),
        repository_oids: [run.commits.base.clone(), run.commits.candidate.clone()],
        action_url: repository_url(action),
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
    .all(exact_sha1)
}

fn canonical_repository(repository: &RepositoryIdentity) -> bool {
    RepositoryIdentity::new(
        repository.host.clone(),
        repository.owner.clone(),
        repository.name.clone(),
    )
    .as_ref()
        == Some(repository)
        && !repository.owner.contains('/')
        && canonical_host(&repository.host)
}

fn exact_sha1(oid: &Oid) -> bool {
    Oid::new(ObjectFormat::Sha1, oid.as_str().to_owned()).as_ref() == Some(oid)
}

fn repository_url(repository: &RepositoryIdentity) -> String {
    format!(
        "https://{}/{}/{}.git",
        repository.host, repository.owner, repository.name
    )
}
