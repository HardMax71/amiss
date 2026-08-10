mod tests;

use std::path::PathBuf;
use std::time::Duration;

use amiss_controller::ProviderError;
use amiss_controller_gitea::{GiteaCommit, GiteaObjectRequest, GiteaObjectResolver, GiteaObjects};
use amiss_controller_service::{GitObjectSource, ResolveWant, ResolvedCommit};
use secrecy::SecretString;

const PREFIX: &str = "amiss-gitea-objects-";
const CANDIDATE_REF: &str = "refs/amiss/gitea/candidate";
const BASE_REF: &str = "refs/amiss/gitea/base";

#[derive(Clone)]
pub(crate) struct GiteaGitObjects {
    source: GitObjectSource,
    repository_id: u64,
}

impl GiteaGitObjects {
    pub(crate) fn new(
        scratch: PathBuf,
        repository_id: u64,
        repository_url: String,
        username: String,
        token: SecretString,
        maximum: Duration,
    ) -> Option<Self> {
        (repository_id > 0)
            .then(|| {
                GitObjectSource::new(scratch, PREFIX, repository_url, username, token, maximum)
            })
            .flatten()
            .map(|source| Self {
                source,
                repository_id,
            })
    }
}

impl GiteaObjectResolver for GiteaGitObjects {
    fn resolve(&self, request: &GiteaObjectRequest) -> Result<GiteaObjects, ProviderError> {
        if request.repository_id != self.repository_id
            || request.repository_url != self.source.repository_url()
        {
            return Err(ProviderError::InvalidResponse);
        }
        let [candidate, base] = self.source.resolve(
            [
                ResolveWant {
                    oid: &request.candidate_commit,
                    reference: CANDIDATE_REF,
                },
                ResolveWant {
                    oid: &request.base_commit,
                    reference: BASE_REF,
                },
            ],
            request.timeout,
        )?;
        Ok(GiteaObjects {
            candidate: commit(candidate),
            base: commit(base),
        })
    }
}

fn commit(resolved: ResolvedCommit) -> GiteaCommit {
    GiteaCommit {
        id: resolved.id,
        tree: resolved.tree,
        parents: resolved.parents,
    }
}
