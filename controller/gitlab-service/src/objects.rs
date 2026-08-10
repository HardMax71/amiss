mod tests;

use std::path::PathBuf;
use std::time::Duration;

use amiss_controller::ProviderError;
use amiss_controller_gitlab::{
    GitLabCommit, GitLabObjectRequest, GitLabObjectResolver, GitLabObjects,
};
use amiss_controller_service::{GitObjectSource, ResolveWant, ResolvedCommit};
use secrecy::SecretString;

const PREFIX: &str = "amiss-gitlab-objects-";
const GATE_REF: &str = "refs/amiss/gitlab/gate";
const BASE_REF: &str = "refs/amiss/gitlab/base";

#[derive(Clone)]
pub(crate) struct GitLabGitObjects {
    source: GitObjectSource,
    project_id: u64,
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
        (project_id > 0)
            .then(|| {
                GitObjectSource::new(scratch, PREFIX, repository_url, username, token, maximum)
            })
            .flatten()
            .map(|source| Self { source, project_id })
    }
}

impl GitLabObjectResolver for GitLabGitObjects {
    fn resolve(&self, request: &GitLabObjectRequest) -> Result<GitLabObjects, ProviderError> {
        if request.project_id != self.project_id
            || request.repository_url != self.source.repository_url()
        {
            return Err(ProviderError::InvalidResponse);
        }
        let [gate, base] = self.source.resolve(
            [
                ResolveWant {
                    oid: &request.gate_commit,
                    reference: GATE_REF,
                },
                ResolveWant {
                    oid: &request.base_commit,
                    reference: BASE_REF,
                },
            ],
            request.timeout,
        )?;
        Ok(GitLabObjects {
            gate: commit(gate),
            base: commit(base),
        })
    }
}

fn commit(resolved: ResolvedCommit) -> GitLabCommit {
    GitLabCommit {
        id: resolved.id,
        tree: resolved.tree,
        parents: resolved.parents,
    }
}
