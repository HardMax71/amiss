use std::fmt;
use std::io::Read as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use amiss_controller::{ProviderError, ProviderIdentity};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use url::Url;

#[path = "../../tests/internal/transport.rs"]
mod tests;

const MAX_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OPERATION_TIMEOUT: Duration = Duration::from_mins(2);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitLabTimeouts {
    connect: Duration,
    operation: Duration,
    response_bytes: usize,
}

impl GitLabTimeouts {
    pub fn new(connect: Duration, operation: Duration, response_bytes: usize) -> Option<Self> {
        (!connect.is_zero()
            && connect <= MAX_CONNECT_TIMEOUT
            && !operation.is_zero()
            && operation <= MAX_OPERATION_TIMEOUT
            && connect <= operation
            && (1..=MAX_RESPONSE_BYTES).contains(&response_bytes))
        .then_some(Self {
            connect,
            operation,
            response_bytes,
        })
    }
}

impl Default for GitLabTimeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            operation: Duration::from_mins(1),
            response_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitLabClientError(&'static str);

impl fmt::Display for GitLabClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the GitLab client configuration is invalid: {}",
            self.0
        )
    }
}

impl std::error::Error for GitLabClientError {}

#[derive(Clone)]
pub(super) struct Transport {
    shared: Arc<Shared>,
}

struct Shared {
    provider: ProviderIdentity,
    base: Url,
    token: SecretString,
    client: Client,
    timeouts: GitLabTimeouts,
}

impl fmt::Debug for Transport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitLabTransport")
            .field("provider", &self.shared.provider)
            .field("base", &self.shared.base)
            .field("token", &"[REDACTED]")
            .field("timeouts", &self.shared.timeouts)
            .finish()
    }
}

#[derive(Clone, Copy)]
pub(super) struct Budget {
    deadline: Instant,
    response_bytes: usize,
}

impl Transport {
    pub(super) fn new(
        provider: ProviderIdentity,
        api_base: &str,
        token: SecretString,
        timeouts: GitLabTimeouts,
    ) -> Result<Self, GitLabClientError> {
        if provider.namespace.as_str() != "gitlab" {
            return Err(GitLabClientError("the provider namespace must be gitlab"));
        }
        let mut base = Url::parse(api_base)
            .map_err(|_defect| GitLabClientError("the API base is not a valid URL"))?;
        if base.path() == "/api/v4" {
            base.set_path("/api/v4/");
        }
        if base.scheme() != "https" {
            return Err(GitLabClientError("the API base must use https"));
        }
        if base.host_str() != Some(provider.instance.as_str()) {
            return Err(GitLabClientError("the API base names the wrong host"));
        }
        if base.port().is_some() {
            return Err(GitLabClientError("the API base must not name a port"));
        }
        if !base.username().is_empty() || base.password().is_some() {
            return Err(GitLabClientError("the API base must not carry credentials"));
        }
        if base.query().is_some() || base.fragment().is_some() {
            return Err(GitLabClientError(
                "the API base must not carry a query or fragment",
            ));
        }
        if base.path() != "/api/v4/" {
            return Err(GitLabClientError(
                "the API base must mount /api/v4 at the root",
            ));
        }
        if token.expose_secret().is_empty() {
            return Err(GitLabClientError("the API token is empty"));
        }
        if HeaderValue::from_str(token.expose_secret()).is_err() {
            return Err(GitLabClientError("the API token is not header-safe"));
        }
        if GitLabTimeouts::new(
            timeouts.connect,
            timeouts.operation,
            timeouts.response_bytes,
        ) != Some(timeouts)
        {
            return Err(GitLabClientError(
                "the transport timeouts are out of bounds",
            ));
        }
        let client = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeouts.connect)
            .build()
            .map_err(|_defect| GitLabClientError("the HTTPS client could not be created"))?;
        Ok(Self {
            shared: Arc::new(Shared {
                provider,
                base,
                token,
                client,
                timeouts,
            }),
        })
    }

    pub(super) fn budget(&self) -> Result<Budget, ProviderError> {
        let deadline = Instant::now()
            .checked_add(self.shared.timeouts.operation)
            .ok_or(ProviderError::Unavailable)?;
        Ok(Budget {
            deadline,
            response_bytes: self.shared.timeouts.response_bytes,
        })
    }

    pub(super) fn provider_instance(&self) -> &str {
        self.shared.provider.instance.as_str()
    }

    pub(super) fn endpoint<'a>(
        &self,
        segments: impl IntoIterator<Item = &'a str>,
    ) -> Result<Url, ProviderError> {
        let mut url = self.shared.base.clone();
        url.path_segments_mut()
            .map_err(|()| ProviderError::InvalidResponse)?
            .pop_if_empty()
            .extend(segments);
        Ok(url)
    }

    pub(super) fn get<T: DeserializeOwned>(
        &self,
        url: Url,
        budget: Budget,
    ) -> Result<(T, Budget), ProviderError> {
        let (value, budget) = self.request(url, budget, false)?;
        value
            .map(|value| (value, budget))
            .ok_or(ProviderError::InvalidResponse)
    }

    pub(super) fn get_optional<T: DeserializeOwned>(
        &self,
        url: Url,
        budget: Budget,
    ) -> Result<(Option<T>, Budget), ProviderError> {
        self.request(url, budget, true)
    }

    fn request<T: DeserializeOwned>(
        &self,
        url: Url,
        budget: Budget,
        missing_allowed: bool,
    ) -> Result<(Option<T>, Budget), ProviderError> {
        let mut token = HeaderValue::from_str(self.shared.token.expose_secret())
            .map_err(|_defect| ProviderError::AuthorizationRevoked)?;
        token.set_sensitive(true);
        let response = self
            .shared
            .client
            .get(url)
            .header("PRIVATE-TOKEN", token)
            .timeout(budget.remaining()?)
            .send()
            .map_err(|error| map_error(&error))?;
        if missing_allowed && response.status() == StatusCode::NOT_FOUND {
            return Ok((None, budget));
        }
        let status = response.status();
        if !status.is_success() {
            return Err(map_status(status));
        }
        let (bytes, budget) = response_bytes(response, budget)?;
        let value =
            serde_json::from_slice(&bytes).map_err(|_defect| ProviderError::InvalidResponse)?;
        Ok((Some(value), budget))
    }
}

impl Budget {
    pub(super) fn remaining(self) -> Result<Duration, ProviderError> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        (!remaining.is_zero())
            .then_some(remaining)
            .ok_or(ProviderError::Unavailable)
    }
}

fn map_status(status: StatusCode) -> ProviderError {
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        ProviderError::AuthorizationRevoked
    } else if status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        ProviderError::Unavailable
    } else {
        ProviderError::InvalidResponse
    }
}

fn response_bytes(response: Response, budget: Budget) -> Result<(Vec<u8>, Budget), ProviderError> {
    let limit = body_limit(response.content_length(), budget)?;
    let mut bytes = Vec::new();
    response
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_defect| ProviderError::Unavailable)?;
    let length = bytes.len();
    Ok((bytes, consume_bytes(budget, length)?))
}

fn body_limit(content_length: Option<u64>, budget: Budget) -> Result<u64, ProviderError> {
    let limit = budget
        .response_bytes
        .checked_add(1)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(ProviderError::InvalidResponse)?;
    content_length
        .is_none_or(|length| length < limit)
        .then_some(limit)
        .ok_or(ProviderError::InvalidResponse)
}

fn consume_bytes(budget: Budget, bytes: usize) -> Result<Budget, ProviderError> {
    let response_bytes = budget
        .response_bytes
        .checked_sub(bytes)
        .ok_or(ProviderError::InvalidResponse)?;
    Ok(Budget {
        response_bytes,
        ..budget
    })
}

fn map_error(error: &reqwest::Error) -> ProviderError {
    if error.is_timeout() || error.is_connect() {
        ProviderError::Unavailable
    } else {
        ProviderError::InvalidResponse
    }
}
