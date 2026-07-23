use std::io::Read as _;
use std::time::Duration;

use amiss_controller::ProviderError;
use reqwest::StatusCode;
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, HeaderValue};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;

use super::super::{GiteaClientError, GiteaTimeouts};
use super::OperationDeadline;

const MAX_API_BASE_BYTES: usize = 2_048;
const MAX_RESPONSE_BYTES: usize = 8 * 1_024 * 1_024;
const GITEA_JSON: &str = "application/json";

pub(super) struct Transport {
    client: Client,
    api_base: String,
    authorization: SecretString,
    operation_timeout: Duration,
}

impl Transport {
    pub(super) fn new(
        provider_instance: &str,
        api_base: &str,
        token: SecretString,
        timeouts: GiteaTimeouts,
    ) -> Result<Self, GiteaClientError> {
        let api_base = validate_api_base(api_base, provider_instance)?;
        let authorization = SecretString::from(format!("token {}", token.expose_secret()));
        drop(token);
        let client = Client::builder()
            .connect_timeout(timeouts.connect)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_defect| GiteaClientError::Client)?;
        Ok(Self {
            client,
            api_base,
            authorization,
            operation_timeout: timeouts.operation,
        })
    }

    pub(super) fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        OperationDeadline::after(self.operation_timeout)
    }

    pub(super) fn get<T: DeserializeOwned>(
        &self,
        route: &str,
        deadline: OperationDeadline,
    ) -> Result<T, ProviderError> {
        self.execute(self.client.get(self.url(route)?), deadline)
    }

    pub(super) fn post<T: DeserializeOwned>(
        &self,
        route: &str,
        body: &impl Serialize,
        deadline: OperationDeadline,
    ) -> Result<T, ProviderError> {
        self.execute(self.client.post(self.url(route)?).json(body), deadline)
    }

    fn execute<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
        deadline: OperationDeadline,
    ) -> Result<T, ProviderError> {
        let mut authorization = HeaderValue::from_str(self.authorization.expose_secret())
            .map_err(|_defect| ProviderError::Authentication)?;
        authorization.set_sensitive(true);
        let response = request
            .header(ACCEPT, GITEA_JSON)
            .header(AUTHORIZATION, authorization)
            .timeout(deadline.remaining()?)
            .send()
            .map_err(|error| map_error(&error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(map_status(status));
        }
        decode_body(response)
    }

    fn url(&self, route: &str) -> Result<Url, ProviderError> {
        if !route.starts_with('/') || route.starts_with("//") {
            return Err(ProviderError::InvalidResponse);
        }
        Url::parse(&format!("{}{route}", self.api_base))
            .map_err(|_defect| ProviderError::InvalidResponse)
    }
}

fn decode_body<T: DeserializeOwned>(response: Response) -> Result<T, ProviderError> {
    let declared = response
        .headers()
        .get(CONTENT_LENGTH)
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|raw| raw.parse::<usize>().ok())
                .ok_or(ProviderError::InvalidResponse)
        })
        .transpose()?;
    let bytes = bounded_bytes(declared, response)?;
    serde_json::from_slice(&bytes).map_err(|_defect| ProviderError::InvalidResponse)
}

fn bounded_bytes(
    declared: Option<usize>,
    reader: impl std::io::Read,
) -> Result<Vec<u8>, ProviderError> {
    if declared.is_some_and(|bytes| bytes > MAX_RESPONSE_BYTES) {
        return Err(ProviderError::InvalidResponse);
    }
    let limit = u64::try_from(MAX_RESPONSE_BYTES)
        .map_err(|_defect| ProviderError::InvalidResponse)?
        .saturating_add(1);
    let mut bytes = Vec::new();
    reader
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_defect| ProviderError::Unavailable)?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(bytes)
}

fn validate_api_base(raw: &str, provider_instance: &str) -> Result<String, GiteaClientError> {
    let configuration = GiteaClientError::Configuration;
    if raw.is_empty() || raw.len() > MAX_API_BASE_BYTES {
        return Err(configuration("the API base length is out of bounds"));
    }
    let url =
        Url::parse(raw).map_err(|_defect| configuration("the API base is not a valid URL"))?;
    if url.scheme() != "https" {
        return Err(configuration("the API base must use https"));
    }
    if url.host_str() != Some(provider_instance) {
        return Err(configuration("the API base names the wrong host"));
    }
    if url.port().is_some() {
        return Err(configuration("the API base must not name a port"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(configuration("the API base must not carry credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(configuration(
            "the API base must not carry a query or fragment",
        ));
    }
    if url.path().trim_end_matches('/') != "/api/v1" {
        return Err(configuration("the API base must mount /api/v1 at the root"));
    }
    let canonical = format!("https://{provider_instance}/api/v1");
    if raw != canonical && raw != format!("{canonical}/") {
        return Err(configuration("the API base is not the canonical form"));
    }
    Ok(canonical)
}

fn map_error(error: &reqwest::Error) -> ProviderError {
    if let Some(status) = error.status() {
        return map_status(status);
    }
    if error.is_builder() || error.is_decode() {
        ProviderError::InvalidResponse
    } else {
        ProviderError::Unavailable
    }
}

fn map_status(status: StatusCode) -> ProviderError {
    match status.as_u16() {
        401 | 403 => ProviderError::AuthorizationRevoked,
        408 | 425 | 429 => ProviderError::Unavailable,
        value if value >= 500 => ProviderError::Unavailable,
        _ => ProviderError::InvalidResponse,
    }
}
