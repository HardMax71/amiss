use std::fmt;
use std::sync::atomic::Ordering;

use amiss_controller::{Acquisition, AcquisitionTarget, ProviderError, RunRequest};
pub use amiss_controller_git::GitFetchBounds;
use amiss_controller_git::{
    ACTION_COMMIT_REF, ExactFetch, ExactWant, GitCredential, REPOSITORY_CANDIDATE_REF,
    REPOSITORY_TARGET_REF, fetch_exact,
};
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use secrecy::SecretString;

const GITHUB_GIT_USERNAME: &str = "x-access-token";

pub trait GitHubTokenSource: Send + Sync {
    /// Returns the short-lived credential for the exact installation named by
    /// the authenticated delivery.
    ///
    /// # Errors
    ///
    /// The installation does not match or GitHub cannot issue a credential.
    fn installation_token(&self, installation_id: u64) -> Result<SecretString, ProviderError>;
}

pub struct GitHubAcquisition<T> {
    tokens: T,
    bounds: GitFetchBounds,
}

impl<T> GitHubAcquisition<T> {
    pub const fn new(tokens: T, bounds: GitFetchBounds) -> Self {
        Self { tokens, bounds }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitHubAcquireError {
    InvalidRequest,
    Credentials,
    Repository,
    Action,
    Cancelled,
}

impl fmt::Display for GitHubAcquireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRequest => "the GitHub acquisition request is inconsistent",
            Self::Credentials => "the GitHub installation credential is unavailable",
            Self::Repository => "the GitHub pull request objects could not be acquired",
            Self::Action => "the pinned action objects could not be acquired",
            Self::Cancelled => "GitHub acquisition was cancelled",
        })
    }
}

impl std::error::Error for GitHubAcquireError {}

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
    let action = &request.plan.execution.action_repository;
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
        && repository.host == provider.instance.as_str()
        && action.host == provider.instance.as_str()
        && canonical_github_repository(repository)
        && canonical_github_repository(action);
    let format_valid = run.refs.forge == ForgeDialect::Github
        && run.object_format == ObjectFormat::Sha1
        && request.provider_run.object_format == ObjectFormat::Sha1
        && request.plan.execution.action_object_format == ObjectFormat::Sha1;
    let binding_valid = request.provider_run == expected_run
        && request.provider_run.candidate_commit == run.commits.candidate
        && [
            &run.commits.base,
            &run.commits.candidate,
            &run.trees.base,
            &run.trees.candidate,
            &request.plan.execution.action_commit_oid,
            &request.plan.execution.action_tree_oid,
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
    if !identity_valid || !format_valid || !binding_valid || !refs_valid {
        return Err(GitHubAcquireError::InvalidRequest);
    }

    let repository_clone_url = repository_url(repository)?;
    let action_url = repository_url(action)?;
    Ok(GitHubFetchPlan {
        installation_id,
        repository_url: repository_clone_url,
        repository_oids: [run.commits.base.clone(), run.commits.candidate.clone()],
        action_url,
        action_oid: request.plan.execution.action_commit_oid.clone(),
    })
}

impl<T: GitHubTokenSource> Acquisition for GitHubAcquisition<T> {
    type Error = GitHubAcquireError;

    fn acquire(
        &mut self,
        request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error> {
        active(&target)?;
        let plan = github_fetch_plan(request)?;
        let token = self
            .tokens
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
        active(&target)
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

fn canonical_github_repository(repository: &RepositoryIdentity) -> bool {
    RepositoryIdentity::new(
        repository.host.clone(),
        repository.owner.clone(),
        repository.name.clone(),
    )
    .as_ref()
        == Some(repository)
        && !repository.owner.contains('/')
        && github_host(&repository.host)
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

fn exact_sha1(oid: &Oid) -> bool {
    Oid::new(ObjectFormat::Sha1, oid.as_str().to_owned()).as_ref() == Some(oid)
}

fn repository_url(repository: &RepositoryIdentity) -> Result<String, GitHubAcquireError> {
    canonical_github_repository(repository)
        .then(|| {
            format!(
                "https://{}/{}/{}.git",
                repository.host, repository.owner, repository.name
            )
        })
        .ok_or(GitHubAcquireError::InvalidRequest)
}
