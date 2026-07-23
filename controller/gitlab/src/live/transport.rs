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
pub struct GitLabClientError;

impl fmt::Display for GitLabClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the GitLab client configuration is invalid")
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
        let mut base = Url::parse(api_base).map_err(|_defect| GitLabClientError)?;
        if base.path() == "/api/v4" {
            base.set_path("/api/v4/");
        }
        let valid = provider.namespace.as_str() == "gitlab"
            && base.scheme() == "https"
            && base.host_str() == Some(provider.instance.as_str())
            && base.port().is_none()
            && base.username().is_empty()
            && base.password().is_none()
            && base.query().is_none()
            && base.fragment().is_none()
            && base.path() == "/api/v4/"
            && !token.expose_secret().is_empty()
            && HeaderValue::from_str(token.expose_secret()).is_ok()
            && GitLabTimeouts::new(
                timeouts.connect,
                timeouts.operation,
                timeouts.response_bytes,
            ) == Some(timeouts);
        if !valid {
            return Err(GitLabClientError);
        }
        let client = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(timeouts.connect)
            .build()
            .map_err(|_defect| GitLabClientError)?;
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
            .map_err(|error| transport_error(&error))?;
        if missing_allowed && response.status() == StatusCode::NOT_FOUND {
            return Ok((None, budget));
        }
        response_status(response.status())?;
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

fn response_status(status: StatusCode) -> Result<(), ProviderError> {
    if status.is_success() {
        Ok(())
    } else if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        Err(ProviderError::AuthorizationRevoked)
    } else if status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
    {
        Err(ProviderError::Unavailable)
    } else {
        Err(ProviderError::InvalidResponse)
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

fn transport_error(error: &reqwest::Error) -> ProviderError {
    if error.is_timeout() || error.is_connect() {
        ProviderError::Unavailable
    } else {
        ProviderError::InvalidResponse
    }
}
