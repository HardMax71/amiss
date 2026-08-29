mod tests;

use std::sync::atomic::Ordering;

use amiss_controller::{
    AcquiredSemanticTemplate, Acquisition, AcquisitionTarget, ProviderError, RunRequest,
    WorkflowArtifactExpectation,
};
pub use amiss_controller_git::GitFetchBounds;
use amiss_controller_git::{
    ACTION_COMMIT_REF, ExactFetch, ExactWant, GitCredential, REPOSITORY_CANDIDATE_REF,
    REPOSITORY_TARGET_REF, fetch_exact,
};
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use secrecy::SecretString;

const GITHUB_GIT_USERNAME: &str = "x-access-token";

pub trait GitHubAcquisitionSource: Send + Sync {
    /// Returns the short-lived credential for the exact installation named by
    /// the authenticated delivery.
    ///
    /// # Errors
    ///
    /// The installation does not match or GitHub cannot issue a credential.
    fn installation_token(&self, installation_id: u64) -> Result<SecretString, ProviderError>;

    /// Reads one planned workflow artifact bound to the candidate commit.
    ///
    /// # Errors
    ///
    /// GitHub cannot prove one exact successful run and artifact, or its bytes do not match the
    /// provider metadata and planned semantic producer.
    fn workflow_artifact(
        &self,
        expectation: &WorkflowArtifactExpectation,
        candidate: &Oid,
    ) -> Result<AcquiredSemanticTemplate, ProviderError>;
}

pub struct GitHubAcquisition<T> {
    source: T,
    bounds: GitFetchBounds,
}

impl<T> GitHubAcquisition<T> {
    pub const fn new(source: T, bounds: GitFetchBounds) -> Self {
        Self { source, bounds }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitHubAcquireError {
    #[error("the GitHub acquisition request is inconsistent")]
    InvalidRequest,
    #[error("the GitHub installation credential is unavailable")]
    Credentials,
    #[error("the GitHub pull request objects could not be acquired")]
    Repository,
    #[error("the pinned action objects could not be acquired")]
    Action,
    #[error("the planned GitHub workflow artifact could not be acquired")]
    Artifact,
    #[error("GitHub acquisition was cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubFetchPlan {
    pub installation_id: u64,
    pub repository_url: String,
    pub repository_oids: [Oid; 2],
    pub action_url: String,
    pub action_oid: Oid,
}

/// Projects a validated provider request into token-free HTTPS fetch inputs.
///
/// # Errors
///
/// The request does not reproduce the authenticated GitHub identity, uses an
/// unsupported object format, or contains a non-GitHub repository or ref.
pub fn github_fetch_plan(request: &RunRequest) -> Result<GitHubFetchPlan, GitHubAcquireError> {
    let run = &request.run;
    let provider = &request.delivery.provider;
    let repository = &run.change.repository;
    let action = request.plan.execution.action_repository();
    let installation_id = request
        .delivery
        .integration
        .as_str()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(GitHubAcquireError::InvalidRequest)?;
    let _change = crate::parse_change_id(run.change.change.as_str())
        .ok_or(GitHubAcquireError::InvalidRequest)?;
    let expected_run = crate::provider_run(
        &request.delivery.integration,
        &run.change,
        &run.commits.candidate,
        &run.refs.candidate,
        &run.refs.target,
    )
    .ok_or(GitHubAcquireError::InvalidRequest)?;

    let identity_valid = provider.namespace.as_str() == "github"
        && request.delivery.provider == run.change.provider
        && repository.host() == provider.instance.as_str()
        && action.host() == provider.instance.as_str()
        && canonical_github_repository(repository)
        && canonical_github_repository(action);
    let format_valid = run.refs.forge == ForgeDialect::Github
        && run.object_format == ObjectFormat::Sha1
        && request.provider_run.object_format == ObjectFormat::Sha1
        && request.plan.execution.action_object_format() == ObjectFormat::Sha1;
    let binding_valid = request.provider_run == expected_run
        && request.provider_run.candidate_commit == run.commits.candidate
        && [
            &run.commits.base,
            &run.commits.candidate,
            &run.trees.base,
            &run.trees.candidate,
            request.plan.execution.action_commit_oid(),
            request.plan.execution.action_tree_oid(),
        ]
        .into_iter()
        .all(exact_sha1);
    let refs_valid = [
        run.refs.candidate.as_str(),
        run.refs.target.as_str(),
        run.refs.default_branch.as_str(),
    ]
    .into_iter()
    .all(|reference| reference.starts_with("refs/heads/"));
    let workflow_artifacts_valid = request
        .plan
        .policy
        .workflow_artifacts
        .iter()
        .all(|artifact| artifact.provider == *provider && artifact.repository == *repository);
    if !identity_valid
        || !format_valid
        || !binding_valid
        || !refs_valid
        || !workflow_artifacts_valid
    {
        return Err(GitHubAcquireError::InvalidRequest);
    }

    let repository_clone_url = repository_url(repository)?;
    let action_url = repository_url(action)?;
    Ok(GitHubFetchPlan {
        installation_id,
        repository_url: repository_clone_url,
        repository_oids: [run.commits.base.clone(), run.commits.candidate.clone()],
        action_url,
        action_oid: request.plan.execution.action_commit_oid().clone(),
    })
}

impl<T: GitHubAcquisitionSource> Acquisition for GitHubAcquisition<T> {
    type Error = GitHubAcquireError;

    fn acquire(
        &mut self,
        request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<Vec<AcquiredSemanticTemplate>, Self::Error> {
        active(&target)?;
        let plan = github_fetch_plan(request)?;
        let mut semantic_templates =
            Vec::with_capacity(request.plan.policy.workflow_artifacts.len());
        for expectation in &request.plan.policy.workflow_artifacts {
            active(&target)?;
            semantic_templates.push(
                self.source
                    .workflow_artifact(expectation, &request.run.commits.candidate)
                    .map_err(|_defect| {
                        fetch_error(target.cancelled.as_ref(), GitHubAcquireError::Artifact)
                    })?,
            );
        }
        active(&target)?;
        let token = self
            .source
            .installation_token(plan.installation_id)
            .map_err(|_defect| GitHubAcquireError::Credentials)?;
        active(&target)?;

        let [repository_target, repository_candidate] = &plan.repository_oids;
        let credential = GitCredential {
            username: GITHUB_GIT_USERNAME,
            password: &token,
        };
        fetch_exact(ExactFetch {
            url: &plan.repository_url,
            wants: &[
                ExactWant {
                    oid: repository_target,
                    reference: REPOSITORY_TARGET_REF,
                },
                ExactWant {
                    oid: repository_candidate,
                    reference: REPOSITORY_CANDIDATE_REF,
                },
            ],
            destination: target.repository,
            credential: Some(credential),
            bounds: self.bounds,
            cancelled: target.cancelled.as_ref(),
        })
        .map_err(|_defect| {
            fetch_error(target.cancelled.as_ref(), GitHubAcquireError::Repository)
        })?;
        active(&target)?;
        fetch_exact(ExactFetch {
            url: &plan.action_url,
            wants: &[ExactWant {
                oid: &plan.action_oid,
                reference: ACTION_COMMIT_REF,
            }],
            destination: target.action,
            credential: Some(credential),
            bounds: self.bounds,
            cancelled: target.cancelled.as_ref(),
        })
        .map_err(|_defect| fetch_error(target.cancelled.as_ref(), GitHubAcquireError::Action))?;
        active(&target).map(|()| semantic_templates)
    }
}

fn active(target: &AcquisitionTarget<'_>) -> Result<(), GitHubAcquireError> {
    (!target.cancelled.load(Ordering::Acquire))
        .then_some(())
        .ok_or(GitHubAcquireError::Cancelled)
}

fn fetch_error(
    cancelled: &std::sync::atomic::AtomicBool,
    error: GitHubAcquireError,
) -> GitHubAcquireError {
    if cancelled.load(Ordering::Acquire) {
        GitHubAcquireError::Cancelled
    } else {
        error
    }
}

pub(crate) fn canonical_github_repository(repository: &RepositoryIdentity) -> bool {
    let Some(rebuilt) = RepositoryIdentity::new(
        repository.host().to_owned(),
        repository.owner().to_owned(),
        repository.name().to_owned(),
    ) else {
        return false;
    };
    if &rebuilt != repository || repository.owner().contains('/') {
        return false;
    }
    github_host(repository.host())
}

pub(crate) fn github_host(host: &str) -> bool {
    host.len() <= 253
        && host.as_bytes().split(|byte| *byte == b'.').all(|label| {
            (1..=63).contains(&label.len())
                && label.first().is_some_and(u8::is_ascii_alphanumeric)
                && label.last().is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .iter()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        })
}

pub(crate) fn exact_sha1(oid: &Oid) -> bool {
    Oid::new(ObjectFormat::Sha1, oid.as_str().to_owned()).as_ref() == Some(oid)
}

fn repository_url(repository: &RepositoryIdentity) -> Result<String, GitHubAcquireError> {
    canonical_github_repository(repository)
        .then(|| {
            format!(
                "https://{}/{}/{}.git",
                repository.host(),
                repository.owner(),
                repository.name()
            )
        })
        .ok_or(GitHubAcquireError::InvalidRequest)
}
