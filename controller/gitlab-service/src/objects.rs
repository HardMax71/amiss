use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_controller_git::{ExactFetch, ExactWant, GitCredential, GitFetchBounds, fetch_exact};
use amiss_controller_gitlab::{
    GitLabCommit, GitLabObjectRequest, GitLabObjectResolver, GitLabObjects,
};
use amiss_git::{GitLimits, GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::model::{ObjectFormat, Oid};
use secrecy::SecretString;

const GATE_REF: &str = "refs/amiss/gitlab/gate";
const BASE_REF: &str = "refs/amiss/gitlab/base";

#[derive(Clone)]
pub(crate) struct GitLabGitObjects {
    scratch: PathBuf,
    project_id: u64,
    repository_url: String,
    username: String,
    token: SecretString,
    maximum: Duration,
}

impl GitLabGitObjects {
    pub(crate) fn new(
        scratch: PathBuf,
        project_id: u64,
        repository_url: String,
        username: String,
        token: SecretString,
        maximum: Duration,
    ) -> Option<Self> {
        let valid_username = !username.is_empty()
            && username.len() <= 256
            && username
                .chars()
                .all(|character| character != ':' && !character.is_control());
        (project_id > 0
            && repository_url.starts_with("https://")
            && valid_username
            && GitFetchBounds::new(maximum).is_some())
        .then_some(Self {
            scratch,
            project_id,
            repository_url,
            username,
            token,
            maximum,
        })
    }
}

impl GitLabObjectResolver for GitLabGitObjects {
    fn resolve(&self, request: &GitLabObjectRequest) -> Result<GitLabObjects, ProviderError> {
        validate_request(request, self.project_id, &self.repository_url)?;
        let seconds = request.timeout.min(self.maximum).as_secs();
        let timeout = Duration::from_secs(seconds);
        let bounds = GitFetchBounds::new(timeout).ok_or(ProviderError::Unavailable)?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ProviderError::Unavailable)?;
        let repository = tempfile::Builder::new()
            .prefix("amiss-gitlab-objects-")
            .tempdir_in(&self.scratch)
            .map_err(|_defect| ProviderError::Unavailable)?;
        let cancelled = AtomicBool::new(false);
        fetch_exact(ExactFetch {
            url: &self.repository_url,
            wants: &[
                ExactWant {
                    oid: &request.gate_commit,
                    reference: GATE_REF,
                },
                ExactWant {
                    oid: &request.base_commit,
                    reference: BASE_REF,
                },
            ],
            destination: repository.path(),
            credential: Some(GitCredential {
                username: &self.username,
                password: &self.token,
            }),
            bounds,
            cancelled: &cancelled,
        })
        .map_err(|_defect| ProviderError::Unavailable)?;
        read_objects(
            repository.path(),
            &request.gate_commit,
            &request.base_commit,
            deadline,
        )
    }
}

fn validate_request(
    request: &GitLabObjectRequest,
    project_id: u64,
    repository_url: &str,
) -> Result<(), ProviderError> {
    let exact = [&request.gate_commit, &request.base_commit]
        .into_iter()
        .all(|oid| Oid::new(ObjectFormat::Sha1, oid.as_str().to_owned()).as_ref() == Some(oid));
    (request.project_id == project_id
        && request.repository_url == repository_url
        && exact
        && !request.timeout.is_zero())
    .then_some(())
    .ok_or(ProviderError::InvalidResponse)
}

fn read_objects(
    root: &Path,
    gate: &Oid,
    base: &Oid,
    deadline: Instant,
) -> Result<GitLabObjects, ProviderError> {
    active(deadline)?;
    let repository = Repository::open(root, ObjectFormat::Sha1)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let gate = read_commit(&repository, &mut resources, gate, deadline)?;
    let base = read_commit(&repository, &mut resources, base, deadline)?;
    active(deadline).map(|()| GitLabObjects { gate, base })
}

fn read_commit(
    repository: &Repository,
    resources: &mut GitResources,
    oid: &Oid,
    deadline: Instant,
) -> Result<GitLabCommit, ProviderError> {
    active(deadline)?;
    let object = repository
        .read_expected(resources, oid, ObjectKind::Commit)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    active(deadline)?;
    let commit = parse_commit(ObjectFormat::Sha1, &object.body)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    active(deadline)?;
    Ok(GitLabCommit {
        id: oid.as_str().to_owned(),
        tree: commit.tree.as_str().to_owned(),
        parents: commit
            .parents
            .into_iter()
            .map(|parent| parent.as_str().to_owned())
            .collect(),
    })
}

fn active(deadline: Instant) -> Result<(), ProviderError> {
    (Instant::now() < deadline)
        .then_some(())
        .ok_or(ProviderError::Unavailable)
}

#[path = "../tests/internal/objects.rs"]
mod tests;
