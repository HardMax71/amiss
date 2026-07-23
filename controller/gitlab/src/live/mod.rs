mod model;
mod refresh;
mod transport;

#[path = "../../tests/internal/live.rs"]
mod tests;

use std::fmt;
use std::sync::Arc;

use amiss_controller::ProviderError;
use url::Url;

use crate::{GitLabObjectRequest, GitLabObjects, GitLabProtection};

use self::transport::{Budget, Transport};
pub use self::transport::{GitLabClientError, GitLabTimeouts};

const PAGE_SIZE: usize = 100;
const MAX_PAGES: u16 = 10;

pub trait GitLabObjectResolver: Send + Sync {
    /// Resolves exact commit and tree objects without trusting the REST body.
    ///
    /// # Errors
    ///
    /// The requested objects cannot be fetched and proven before `timeout`.
    fn resolve(&self, request: &GitLabObjectRequest) -> Result<GitLabObjects, ProviderError>;
}

#[derive(Clone)]
pub struct GitLabClient {
    transport: Transport,
    objects: Arc<dyn GitLabObjectResolver>,
}

impl fmt::Debug for GitLabClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitLabClient")
            .field("transport", &self.transport)
            .field("objects", &"[resolver]")
            .finish()
    }
}

impl GitLabClient {
    /// Builds a strict HTTPS GitLab REST client with an independent Git resolver.
    ///
    /// # Errors
    ///
    /// The provider, base URL, token, timeout, or response ceiling is invalid.
    pub fn new(
        provider: amiss_controller::ProviderIdentity,
        api_base: &str,
        token: secrecy::SecretString,
        timeouts: GitLabTimeouts,
        objects: Arc<dyn GitLabObjectResolver>,
    ) -> Result<Self, GitLabClientError> {
        Ok(Self {
            transport: Transport::new(provider, api_base, token, timeouts)?,
            objects,
        })
    }

    fn protections(
        &self,
        project_id: &str,
        mut budget: Budget,
    ) -> Result<(Vec<GitLabProtection>, Budget), ProviderError> {
        let mut protections = Vec::new();
        for page in 1..=MAX_PAGES {
            let mut url =
                self.transport
                    .endpoint(["projects", project_id, "protected_branches"])?;
            url.query_pairs_mut()
                .append_pair("per_page", &PAGE_SIZE.to_string())
                .append_pair("page", &page.to_string());
            let (batch, next) = self.transport.get::<Vec<GitLabProtection>>(url, budget)?;
            budget = next;
            let complete = page_complete(page, batch.len())?;
            protections.extend(batch);
            if complete {
                return Ok((protections, budget));
            }
        }
        Err(ProviderError::InvalidResponse)
    }

    fn endpoint(
        &self,
        project_id: &str,
        tail: impl IntoIterator<Item = String>,
    ) -> Result<Url, ProviderError> {
        let segments = ["projects".to_owned(), project_id.to_owned()]
            .into_iter()
            .chain(tail)
            .collect::<Vec<_>>();
        self.transport.endpoint(segments.iter().map(String::as_str))
    }
}

fn page_complete(page: u16, rows: usize) -> Result<bool, ProviderError> {
    if rows > PAGE_SIZE || page == MAX_PAGES && rows == PAGE_SIZE {
        Err(ProviderError::InvalidResponse)
    } else {
        Ok(rows < PAGE_SIZE)
    }
}
