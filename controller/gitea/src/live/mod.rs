mod model;
mod publication;
mod refresh;
mod rest;

#[path = "../../tests/internal/live.rs"]
mod tests;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{ChangeSnapshot, ProviderError, ProviderIdentity, Publication};
use amiss_wire::controls::valid_required_status_name;
use secrecy::{ExposeSecret as _, SecretString};

use crate::identity::canonical_host;
use crate::{DedicatedReviewer, GiteaApi, GiteaPullRequest};

use self::publication::{
    PublicationDecision, publication_decision, publishable, validate_created, validate_publication,
};
use self::refresh::{publication_target_is_current, snapshot, validate_request};
use self::rest::{GiteaRest, HttpRest};

const MAX_TOKEN_BYTES: usize = 4_096;
const MIN_TOKEN_BYTES: usize = 16;
const MAX_IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GiteaTimeouts {
    connect: Duration,
    operation: Duration,
}

impl GiteaTimeouts {
    pub fn new(connect: Duration, operation: Duration) -> Option<Self> {
        let connect_valid = !connect.is_zero() && connect <= MAX_IO_TIMEOUT;
        let operation_valid =
            !operation.is_zero() && operation <= MAX_IO_TIMEOUT && connect <= operation;
        (connect_valid && operation_valid).then_some(Self { connect, operation })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiteaClientError {
    Configuration(&'static str),
    Client,
}

impl fmt::Display for GiteaClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(reason) => {
                write!(
                    formatter,
                    "the Gitea-family configuration is invalid: {reason}"
                )
            }
            Self::Client => {
                formatter.write_str("the Gitea-family HTTPS client could not be created")
            }
        }
    }
}

impl std::error::Error for GiteaClientError {}

#[derive(Clone)]
pub struct GiteaClient {
    client: Arc<Client<HttpRest>>,
}

impl GiteaClient {
    /// Creates one exact Gitea-family dedicated-reviewer client.
    ///
    /// # Errors
    ///
    /// The provider, reviewer, token, API base, review name, or transport is invalid.
    pub fn new(
        provider: ProviderIdentity,
        reviewer: DedicatedReviewer,
        token: String,
        api_base: &str,
        review_name: String,
        timeouts: GiteaTimeouts,
    ) -> Result<Self, GiteaClientError> {
        let token = SecretString::from(token);
        let configuration = GiteaClientError::Configuration;
        if !canonical_host(provider.instance.as_str()) {
            return Err(configuration(
                "the provider instance is not a canonical host",
            ));
        }
        if DedicatedReviewer::new(reviewer.id, reviewer.login.clone()).as_ref() != Some(&reviewer) {
            return Err(configuration("the reviewer identity is not canonical"));
        }
        if !(MIN_TOKEN_BYTES..=MAX_TOKEN_BYTES).contains(&token.expose_secret().len()) {
            return Err(configuration("the reviewer token size is out of bounds"));
        }
        if !valid_required_status_name(&review_name) {
            return Err(configuration("the review label is not a valid status name"));
        }
        let rest = HttpRest::new(provider.instance.as_str(), api_base, token, timeouts)?;
        Ok(Self {
            client: Arc::new(Client {
                config: Config {
                    provider,
                    reviewer,
                    review_name,
                },
                rest,
            }),
        })
    }
}

impl GiteaApi for GiteaClient {
    fn refresh(&self, pull_request: GiteaPullRequest<'_>) -> Result<ChangeSnapshot, ProviderError> {
        self.client.refresh(pull_request)
    }

    fn publish(
        &self,
        pull_request: GiteaPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.client.publish(pull_request, publication)
    }
}

struct Client<R> {
    config: Config,
    rest: R,
}

impl<R: GiteaRest> Client<R> {
    fn refresh(&self, pull_request: GiteaPullRequest<'_>) -> Result<ChangeSnapshot, ProviderError> {
        validate_request(&self.config, pull_request)?;
        let deadline = self.rest.deadline()?;
        let data = self
            .rest
            .refresh_data(&self.config, pull_request, deadline)?;
        snapshot(&self.config, pull_request, &data)
    }

    fn publish(
        &self,
        pull_request: GiteaPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        validate_request(&self.config, pull_request)?;
        validate_publication(&self.config, pull_request, publication)?;
        let deadline = self.rest.deadline()?;
        let data = self
            .rest
            .refresh_data(&self.config, pull_request, deadline)?;
        let state = publication_target_is_current(&self.config, pull_request, publication, &data)?;
        if !publishable(state)? {
            return Ok(());
        }
        match publication_decision(&self.config, publication, &data.reviews)? {
            PublicationDecision::Reuse => Ok(()),
            PublicationDecision::Create(expected) => {
                let created = self.rest.create_review(pull_request, &expected, deadline)?;
                validate_created(&self.config, &expected, &created)
            }
        }
    }
}

#[derive(Clone)]
struct Config {
    provider: ProviderIdentity,
    reviewer: DedicatedReviewer,
    review_name: String,
}
