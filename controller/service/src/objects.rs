use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_controller_git::{ExactFetch, ExactWant, GitCredential, GitFetchBounds, fetch_exact};
use amiss_git::{GitLimits, GitResources, ObjectKind, Repository, parse_commit};
use amiss_wire::model::{ObjectFormat, Oid};
use secrecy::SecretString;

const MAX_USERNAME_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCommit {
    pub id: String,
    pub tree: String,
    pub parents: Vec<String>,
}

#[derive(Clone, Copy)]
pub struct ResolveWant<'a> {
    pub oid: &'a Oid,
    pub reference: &'a str,
}

/// Reads exact commit and tree objects from the provider's Git remote, so a
/// lane never has to trust a REST body for an object name.
#[derive(Clone)]
pub struct GitObjectSource {
    scratch: PathBuf,
    prefix: &'static str,
    repository_url: String,
    username: String,
    token: SecretString,
    maximum: Duration,
}

impl GitObjectSource {
    #[must_use]
    pub fn new(
        scratch: PathBuf,
        prefix: &'static str,
        repository_url: String,
        username: String,
        token: SecretString,
        maximum: Duration,
    ) -> Option<Self> {
        let valid_username = !username.is_empty()
            && username.len() <= MAX_USERNAME_BYTES
            && username
                .chars()
                .all(|character| character != ':' && !character.is_control());
        (repository_url.starts_with("https://")
            && !prefix.is_empty()
            && valid_username
            && !secrecy::ExposeSecret::expose_secret(&token).is_empty()
            && GitFetchBounds::new(maximum).is_some())
        .then_some(Self {
            scratch,
            prefix,
            repository_url,
            username,
            token,
            maximum,
        })
    }

    #[must_use]
    pub fn repository_url(&self) -> &str {
        &self.repository_url
    }

    /// Fetches each wanted commit and reads its proven commit and tree names.
    ///
    /// # Errors
    ///
    /// A want is inexact, or the objects cannot be fetched and read before the
    /// timeout.
    pub fn resolve<const N: usize>(
        &self,
        wants: [ResolveWant<'_>; N],
        timeout: Duration,
    ) -> Result<[ResolvedCommit; N], ProviderError> {
        let exact = wants.iter().all(|want| {
            Oid::new(ObjectFormat::Sha1, want.oid.as_str().to_owned()).as_ref() == Some(want.oid)
                && !want.reference.is_empty()
        });
        if !exact || timeout.is_zero() {
            return Err(ProviderError::InvalidResponse);
        }
        let timeout = Duration::from_secs(timeout.min(self.maximum).as_secs());
        let bounds = GitFetchBounds::new(timeout).ok_or(ProviderError::Unavailable)?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ProviderError::Unavailable)?;
        let repository = tempfile::Builder::new()
            .prefix(self.prefix)
            .tempdir_in(&self.scratch)
            .map_err(|_defect| ProviderError::Unavailable)?;
        let cancelled = AtomicBool::new(false);
        let fetch: Vec<ExactWant<'_>> = wants
            .iter()
            .map(|want| ExactWant {
                oid: want.oid,
                reference: want.reference,
            })
            .collect();
        fetch_exact(ExactFetch {
            url: &self.repository_url,
            wants: &fetch,
            destination: repository.path(),
            credential: Some(GitCredential {
                username: &self.username,
                password: &self.token,
            }),
            bounds,
            cancelled: &cancelled,
        })
        .map_err(|_defect| ProviderError::Unavailable)?;
        read_commits(repository.path(), wants, deadline)
    }
}

fn read_commits<const N: usize>(
    root: &Path,
    wants: [ResolveWant<'_>; N],
    deadline: Instant,
) -> Result<[ResolvedCommit; N], ProviderError> {
    active(deadline)?;
    let repository = Repository::open(root, ObjectFormat::Sha1)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let mut read = Vec::with_capacity(N);
    for want in wants {
        read.push(read_commit(
            &repository,
            &mut resources,
            want.oid,
            deadline,
        )?);
    }
    active(deadline)?;
    read.try_into()
        .map_err(|_defect: Vec<ResolvedCommit>| ProviderError::Unavailable)
}

fn read_commit(
    repository: &Repository,
    resources: &mut GitResources,
    oid: &Oid,
    deadline: Instant,
) -> Result<ResolvedCommit, ProviderError> {
    active(deadline)?;
    let object = repository
        .read_expected(resources, oid, ObjectKind::Commit)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    active(deadline)?;
    let commit = parse_commit(ObjectFormat::Sha1, &object.body)
        .map_err(|_defect| ProviderError::InvalidResponse)?;
    active(deadline)?;
    Ok(ResolvedCommit {
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
