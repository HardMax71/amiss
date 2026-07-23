use std::future::Future;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use bytes::Bytes;
use http::Response;
use http::header::{ACCEPT, HeaderName};
use http_body::Body;
use http_body_util::BodyExt as _;
use jsonwebtoken::EncodingKey;
use octocrab::models::{AppId, InstallationId};
use octocrab::{Octocrab, OctocrabBuilder};
use secrecy::{ExposeSecret as _, SecretSlice, SecretString};
use tokio::runtime::Runtime;
use url::Url;

use amiss_controller::ProviderError;

use super::super::{GitHubClientError, GitHubTimeouts};
use super::OperationDeadline;

#[path = "../../../tests/internal/transport.rs"]
mod tests;

const MAX_API_BASE_BYTES: usize = 2_048;
const MAX_RESPONSE_BYTES: usize = 8 * 1_024 * 1_024;
const GITHUB_API_VERSION: &str = "2022-11-28";
const GITHUB_JSON: &str = "application/vnd.github+json";

pub(super) struct Transport {
    runtime: Arc<Runtime>,
    crab: Arc<Octocrab>,
    operation_timeout: Duration,
}

impl Transport {
    pub(super) fn new(
        app_id: u64,
        installation_id: u64,
        private_key: SecretSlice<u8>,
        provider_instance: &str,
        api_base: &str,
        timeouts: GitHubTimeouts,
    ) -> Result<Self, GitHubClientError> {
        validate_api_base(api_base, provider_instance)?;
        let key = EncodingKey::from_rsa_pem(private_key.expose_secret())
            .map_err(|_defect| GitHubClientError::Configuration)?;
        drop(private_key);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|_defect| GitHubClientError::Runtime)?,
        );
        let builder = OctocrabBuilder::new()
            .base_uri(api_base)
            .map_err(|_defect| GitHubClientError::Configuration)?
            .set_connect_timeout(Some(timeouts.connect))
            .set_read_timeout(Some(timeouts.read))
            .set_write_timeout(Some(timeouts.write))
            .add_header(ACCEPT, GITHUB_JSON.to_owned())
            .add_header(
                HeaderName::from_static("x-github-api-version"),
                GITHUB_API_VERSION.to_owned(),
            )
            .app(AppId(app_id), key);
        let app = {
            let _runtime = runtime.enter();
            builder
                .build()
                .map_err(|_defect| GitHubClientError::Client)?
        };
        let crab = app
            .installation(InstallationId(installation_id))
            .map_err(|_defect| GitHubClientError::Client)?;
        Ok(Self {
            runtime,
            crab: Arc::new(crab),
            operation_timeout: timeouts.operation,
        })
    }

    pub(super) fn installation_access_token(&self) -> Result<SecretString, ProviderError> {
        let crab = Arc::clone(&self.crab);
        let deadline = self.deadline()?;
        self.execute(
            async move {
                crab.installation_token()
                    .await
                    .map_err(|error| map_error(&error))
            },
            deadline,
        )
    }

    pub(super) fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        OperationDeadline::after(self.operation_timeout)
    }

    pub(super) fn get<T>(
        &self,
        route: String,
        deadline: OperationDeadline,
    ) -> Result<T, ProviderError>
    where
        T: serde::de::DeserializeOwned + Send + 'static,
    {
        self.request(deadline, |crab| async move { crab._get(route).await })
    }

    pub(super) fn request<T, B, F, Fut>(
        &self,
        deadline: OperationDeadline,
        build: F,
    ) -> Result<T, ProviderError>
    where
        T: serde::de::DeserializeOwned + Send + 'static,
        B: Body<Data = Bytes> + Unpin + Send + 'static,
        F: FnOnce(Arc<Octocrab>) -> Fut,
        Fut: Future<Output = Result<Response<B>, octocrab::Error>> + Send + 'static,
    {
        let request = build(Arc::clone(&self.crab));
        self.execute(
            async move {
                let response = request.await.map_err(|error| map_error(&error))?;
                decode_json(response).await
            },
            deadline,
        )
    }

    fn execute<T, F>(&self, future: F, deadline: OperationDeadline) -> Result<T, ProviderError>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, ProviderError>> + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        let timeout = deadline.remaining()?;
        let _task = self.runtime.spawn(async move {
            let result = tokio::time::timeout(timeout, future)
                .await
                .map_err(|_elapsed| ProviderError::Unavailable)
                .and_then(std::convert::identity);
            let _ignored = sender.send(result);
        });
        receiver
            .recv_timeout(timeout)
            .map_err(|_defect| ProviderError::Unavailable)?
    }
}

async fn decode_json<T, B>(response: Response<B>) -> Result<T, ProviderError>
where
    T: serde::de::DeserializeOwned,
    B: Body<Data = Bytes> + Unpin,
{
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(map_status(status));
    }
    let declared = response
        .headers()
        .get("content-length")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or(ProviderError::InvalidResponse)
        })
        .transpose()?;
    if declared.is_some_and(|bytes| bytes > MAX_RESPONSE_BYTES) {
        return Err(ProviderError::InvalidResponse);
    }
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_defect| ProviderError::Unavailable)?;
        if let Ok(data) = frame.into_data() {
            if data.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
                return Err(ProviderError::InvalidResponse);
            }
            bytes.extend_from_slice(&data);
        }
    }
    serde_json::from_slice(&bytes).map_err(|_defect| ProviderError::InvalidResponse)
}

fn validate_api_base(raw: &str, provider_instance: &str) -> Result<(), GitHubClientError> {
    let url = Url::parse(raw).map_err(|_defect| GitHubClientError::Configuration)?;
    let no_port = raw
        .parse::<http::Uri>()
        .ok()
        .and_then(|uri| uri.authority().map(|authority| authority.port().is_none()))
        .unwrap_or(false);
    let expected_host = if provider_instance == "github.com" {
        "api.github.com"
    } else {
        provider_instance
    };
    let valid = !raw.is_empty()
        && raw.len() <= MAX_API_BASE_BYTES
        && url.scheme() == "https"
        && url.host_str() == Some(expected_host)
        && no_port
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    valid.then_some(()).ok_or(GitHubClientError::Configuration)
}

fn map_error(error: &octocrab::Error) -> ProviderError {
    if let octocrab::Error::GitHub { source, .. } = error {
        return map_status(source.status_code.as_u16());
    }
    if let octocrab::Error::JWT { .. }
    | octocrab::Error::Installation { .. }
    | octocrab::Error::InstallationTokenInvalidAuth { .. } = error
    {
        return ProviderError::Authentication;
    }
    if let octocrab::Error::UriParse { .. }
    | octocrab::Error::Uri { .. }
    | octocrab::Error::InvalidHeaderValue { .. }
    | octocrab::Error::Http { .. }
    | octocrab::Error::InvalidUtf8 { .. }
    | octocrab::Error::SerdeUrlEncoded { .. }
    | octocrab::Error::Serde { .. }
    | octocrab::Error::Json { .. }
    | octocrab::Error::Graphql { .. }
    | octocrab::Error::Other { .. } = error
    {
        return ProviderError::InvalidResponse;
    }
    ProviderError::Unavailable
}

fn map_status(status: u16) -> ProviderError {
    if matches!(status, 401 | 403) {
        ProviderError::AuthorizationRevoked
    } else if matches!(status, 408 | 425 | 429) || status >= 500 {
        ProviderError::Unavailable
    } else {
        ProviderError::InvalidResponse
    }
}
