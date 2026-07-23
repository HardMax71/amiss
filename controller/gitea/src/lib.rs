#![forbid(unsafe_code)]

mod adapter;
mod fetch_plan;
mod identity;
mod live;
mod source;

use amiss_controller::{ChangeLocator, ChangeSnapshot, ProviderError, Publication};
use amiss_wire::model::Oid;

pub use adapter::GiteaPullRequestAdapter;
pub use fetch_plan::{GiteaFetchPlan, GiteaPlanError, gitea_fetch_plan};
pub use live::{GiteaClient, GiteaClientError, GiteaTimeouts};
pub use source::GiteaPullRequestSource;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DedicatedReviewer {
    pub id: u64,
    pub login: String,
}

impl DedicatedReviewer {
    pub fn new(id: u64, login: String) -> Option<Self> {
        let canonical = identity::canonical_segment(&login)?;
        (id > 0 && canonical == login).then_some(Self { id, login })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GiteaPullRequest<'a> {
    pub change: &'a ChangeLocator,
    pub reviewer_id: u64,
    pub repository_id: u64,
    pub repository_owner: &'a str,
    pub repository_name: &'a str,
    pub pull_request_id: u64,
    pub number: u64,
    pub candidate_commit: &'a Oid,
}

pub trait GiteaApi: Send + Sync {
    /// Fetches the current state of the exact authenticated pull request.
    ///
    /// # Errors
    ///
    /// The provider state cannot be obtained, authenticated, or proven.
    fn refresh(&self, pull_request: GiteaPullRequest<'_>) -> Result<ChangeSnapshot, ProviderError>;

    /// Reconciles one staged result as the dedicated reviewer's exact review.
    ///
    /// # Errors
    ///
    /// The provider does not confirm the exact review.
    fn publish(
        &self,
        pull_request: GiteaPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError>;
}
