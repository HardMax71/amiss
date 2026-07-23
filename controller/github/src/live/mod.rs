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
use secrecy::{ExposeSecret as _, SecretSlice, SecretString};

use crate::{GitHubApi, GitHubPullRequest, GitHubTokenSource};

use self::publication::{
    PublicationDecision, publication_decision, validate_created, validate_publication,
};
use self::refresh::{publication_target_is_current, snapshot, validate_request};
use self::rest::{GitHubRest, HttpRest};

const MAX_PRIVATE_KEY_BYTES: usize = 65_536;
const MIN_PRIVATE_KEY_BYTES: usize = 512;
const MAX_IO_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitHubTimeouts {
    connect: Duration,
    read: Duration,
    write: Duration,
    operation: Duration,
}

impl GitHubTimeouts {
    /// Bounds transport phases and one complete `refresh` or `publish` call.
    ///
    /// The operation timeout must be no longer than 30 seconds and must cover
    /// every transport phase. Configure it below the controller ledger lease.
    pub fn new(
        connect: Duration,
        read: Duration,
        write: Duration,
        operation: Duration,
    ) -> Option<Self> {
        let phases = [connect, read, write];
        let phases_valid = phases
            .into_iter()
            .all(|timeout| !timeout.is_zero() && timeout <= MAX_IO_TIMEOUT);
        let operation_valid = !operation.is_zero()
            && operation <= MAX_IO_TIMEOUT
            && phases.into_iter().all(|timeout| timeout <= operation);
        (phases_valid && operation_valid).then_some(Self {
            connect,
            read,
            write,
            operation,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitHubClientError {
    Configuration(&'static str),
    Client,
}

impl fmt::Display for GitHubClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(reason) => {
                write!(
                    formatter,
                    "the GitHub App configuration is invalid: {reason}"
                )
            }
            Self::Client => formatter.write_str("the GitHub App client could not be created"),
        }
    }
}

impl std::error::Error for GitHubClientError {}

#[derive(Clone)]
pub struct GitHubApp {
    client: Arc<Client<HttpRest>>,
}

impl GitHubApp {
    /// Creates one exact GitHub App installation client.
    ///
    /// The PEM is moved into redacted, zeroizing storage before validation.
    ///
    /// # Errors
    ///
    /// The provider, IDs, key, API base, status name, or client
    /// configuration is invalid.
    pub fn new(
        provider: ProviderIdentity,
        app_id: u64,
        installation_id: u64,
        private_key_pem: Vec<u8>,
        api_base: &str,
        required_status_name: String,
        timeouts: GitHubTimeouts,
    ) -> Result<Self, GitHubClientError> {
        let private_key = SecretSlice::from(private_key_pem);
        let configuration = GitHubClientError::Configuration;
        if provider.namespace.as_str() != "github" {
            return Err(configuration("the provider namespace must be github"));
        }
        if !crate::acquisition::github_host(provider.instance.as_str()) {
            return Err(configuration("the provider instance is not a GitHub host"));
        }
        if app_id == 0 || installation_id == 0 {
            return Err(configuration(
                "the App and installation IDs must be positive",
            ));
        }
        if !(MIN_PRIVATE_KEY_BYTES..=MAX_PRIVATE_KEY_BYTES)
            .contains(&private_key.expose_secret().len())
        {
            return Err(configuration("the App private key size is out of bounds"));
        }
        if !valid_required_status_name(&required_status_name) {
            return Err(configuration("the required status name is invalid"));
        }
        let rest = HttpRest::new(
            app_id,
            installation_id,
            private_key,
            provider.instance.as_str(),
            api_base,
            timeouts,
        )?;
        Ok(Self {
            client: Arc::new(Client {
                config: Config {
                    provider,
                    app_id,
                    installation_id,
                    required_status_name,
                },
                rest,
            }),
        })
    }

    /// Returns the cached, redacted installation credential for a scoped
    /// HTTPS Git acquisition callback.
    ///
    /// # Errors
    ///
    /// GitHub cannot authenticate this exact installation.
    pub fn installation_access_token(&self) -> Result<SecretString, ProviderError> {
        self.client.rest.installation_access_token()
    }
}

impl GitHubApi for GitHubApp {
    fn refresh(
        &self,
        pull_request: GitHubPullRequest<'_>,
    ) -> Result<ChangeSnapshot, ProviderError> {
        self.client.refresh(pull_request)
    }

    fn publish(
        &self,
        pull_request: GitHubPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        self.client.publish(pull_request, publication)
    }
}

impl GitHubTokenSource for GitHubApp {
    fn installation_token(&self, installation_id: u64) -> Result<SecretString, ProviderError> {
        (installation_id == self.client.config.installation_id)
            .then_some(())
            .ok_or(ProviderError::Authentication)?;
        self.installation_access_token()
    }
}

struct Client<R> {
    config: Config,
    rest: R,
}

impl<R: GitHubRest> Client<R> {
    fn refresh(
        &self,
        pull_request: GitHubPullRequest<'_>,
    ) -> Result<ChangeSnapshot, ProviderError> {
        validate_request(&self.config, pull_request)?;
        let deadline = self.rest.deadline()?;
        let data = self.rest.refresh_data(pull_request, deadline)?;
        snapshot(&self.config, pull_request, &data)
    }

    fn publish(
        &self,
        pull_request: GitHubPullRequest<'_>,
        publication: &Publication,
    ) -> Result<(), ProviderError> {
        validate_request(&self.config, pull_request)?;
        validate_publication(&self.config, pull_request, publication)?;
        let deadline = self.rest.deadline()?;
        let authoritative = self.rest.pull_request(pull_request, deadline)?;
        if !publication_target_is_current(&self.config, pull_request, publication, &authoritative)?
        {
            return Ok(());
        }
        let runs = self.rest.check_runs(
            pull_request,
            &publication.gate_commit,
            self.config.app_id,
            &self.config.required_status_name,
            deadline,
        )?;
        match publication_decision(&self.config, publication, &runs)? {
            PublicationDecision::Reuse => Ok(()),
            PublicationDecision::Create(expected) => {
                let created = self
                    .rest
                    .create_check_run(pull_request, &expected, deadline)?;
                validate_created(&self.config, &expected, &created)
            }
        }
    }
}

#[derive(Clone)]
struct Config {
    provider: ProviderIdentity,
    app_id: u64,
    installation_id: u64,
    required_status_name: String,
}
