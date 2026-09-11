mod tests;

use std::time::Duration;

use amiss_controller::{
    ForgeFact, ForgeNegative, ProviderError, decode_bounded_json, send_request,
};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret as _, SecretString};
use serde::de::DeserializeOwned;
use url::Url;

use super::super::{GiteaClientError, GiteaTimeouts};
use super::OperationDeadline;

const MAX_API_BASE_BYTES: usize = 2_048;
pub(super) const MAX_RESPONSE_BYTES: usize = 8 * 1_024 * 1_024;
const GITEA_JSON: &str = "application/json";

pub(super) struct Transport {
    pub(super) client: Client,
    api_base: String,
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
        let mut token_header =
            HeaderValue::from_str(authorization.expose_secret()).map_err(|_defect| {
                GiteaClientError::Configuration("the token is not a valid HTTP header")
            })?;
        drop(authorization);
        token_header.set_sensitive(true);
        let client = Client::builder()
            .default_headers(HeaderMap::from_iter([
                (ACCEPT, HeaderValue::from_static(GITEA_JSON)),
                (AUTHORIZATION, token_header),
            ]))
            .connect_timeout(timeouts.connect)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_defect| GiteaClientError::Client)?;
        Ok(Self {
            client,
            api_base,
            operation_timeout: timeouts.operation,
        })
    }

    pub(super) fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        OperationDeadline::after(self.operation_timeout)
    }

    pub(super) fn get<T>(
        &self,
        route: &str,
        deadline: OperationDeadline,
        handle: impl FnOnce(Response) -> Result<T, ProviderError>,
    ) -> Result<T, ProviderError> {
        handle(send_request(
            self.client.get(self.url(route)?),
            deadline,
            map_error,
        )?)
    }

    pub(super) fn url(&self, route: &str) -> Result<Url, ProviderError> {
        if !route.starts_with('/') || route.starts_with("//") {
            return Err(ProviderError::InvalidResponse);
        }
        Url::parse(&format!("{}{route}", self.api_base))
            .map_err(|_defect| ProviderError::InvalidResponse)
    }
}

pub(super) fn decode_body<T: DeserializeOwned>(response: Response) -> Result<T, ProviderError> {
    let status = response.status();
    if !status.is_success() {
        return Err(map_status(status));
    }
    let declared = response.content_length();
    decode_bounded_json(response, declared, MAX_RESPONSE_BYTES, |bytes| {
        serde_json::from_slice(bytes)
    })
    .map(|(value, _length)| value)
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

pub(super) fn map_error(error: &reqwest::Error) -> ProviderError {
    if let Some(status) = error.status() {
        return map_status(status);
    }
    if error.is_builder() || error.is_decode() {
        ProviderError::InvalidResponse
    } else {
        ProviderError::Unavailable
    }
}

/// The verification statuses that are facts: 404 and 422 are the absence of
/// what the route names, a 403 is a standing refusal since Gitea carries no
/// rate-limit signal, and everything else classifies as a data call would.
pub(super) fn classified(response: Response) -> Result<ForgeFact<Response>, ProviderError> {
    let status = response.status();
    match status.as_u16() {
        200..300 => Ok(Ok(response)),
        404 | 422 => Ok(Err(ForgeNegative::Missing)),
        403 => Ok(Err(ForgeNegative::Denied)),
        _ => Err(map_status(status)),
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
