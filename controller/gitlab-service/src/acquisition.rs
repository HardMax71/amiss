use amiss_controller::RunRequest;
use amiss_controller_git::{GitAcquisition, GitAcquisitionPlan, GitFetchBounds, GitRemote};
use amiss_controller_gitlab::{GitLabFetchPlan, GitLabPlanError, gitlab_fetch_plan};
use secrecy::{ExposeSecret as _, SecretString};

type PlanBuilder =
    Box<dyn FnMut(&RunRequest) -> Result<GitAcquisitionPlan, GitLabPlanError> + Send>;

pub(crate) type GitLabAcquisition = GitAcquisition<PlanBuilder>;

pub(crate) fn gitlab_acquisition(
    bounds: GitFetchBounds,
    project_id: u64,
    username: String,
    token: SecretString,
) -> GitLabAcquisition {
    let build: PlanBuilder = Box::new(move |request| {
        let plan = gitlab_fetch_plan(request)?;
        (plan.project_id == project_id)
            .then(|| acquisition_plan(plan, &username, &token))
            .ok_or(GitLabPlanError)
    });
    GitAcquisition {
        bounds,
        plan: build,
    }
}

fn acquisition_plan(
    plan: GitLabFetchPlan,
    username: &str,
    token: &SecretString,
) -> GitAcquisitionPlan {
    GitAcquisitionPlan {
        repository: remote(plan.repository_url, username, token),
        repository_oids: plan.repository_oids,
        action: remote(plan.action_url, username, token),
        action_oid: plan.action_oid,
    }
}

fn remote(url: String, username: &str, token: &SecretString) -> GitRemote {
    GitRemote {
        url,
        username: username.to_owned(),
        password: SecretString::from(token.expose_secret().to_owned()),
    }
}
