use std::io::Read as _;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, HeaderName, HeaderValue};
use secrecy::{ExposeSecret as _, SecretSlice, SecretString};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
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
const USER_AGENT: &str = "amiss-controller-github";
const JWT_BACKDATE_SECONDS: u64 = 60;
const JWT_LIFETIME_SECONDS: u64 = 540;
const TOKEN_REUSE: Duration = Duration::from_mins(5);

pub(super) struct Transport {
    client: Client,
    api_base: String,
    app: AppCredential,
    minted: Mutex<Option<MintedToken>>,
    operation_timeout: Duration,
}

struct AppCredential {
    key: EncodingKey,
    app_id: u64,
    installation_id: u64,
}

struct MintedToken {
    token: SecretString,
    minted_at: Instant,
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
        let api_base = validate_api_base(api_base, provider_instance)?;
        let key = EncodingKey::from_rsa_pem(private_key.expose_secret())
            .map_err(|_defect| GitHubClientError::Configuration)?;
        drop(private_key);
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(timeouts.connect)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|_defect| GitHubClientError::Client)?;
        Ok(Self {
            client,
            api_base,
            app: AppCredential {
                key,
                app_id,
                installation_id,
            },
            minted: Mutex::new(None),
            operation_timeout: timeouts.operation,
        })
    }

    pub(super) fn installation_access_token(&self) -> Result<SecretString, ProviderError> {
        let deadline = self.deadline()?;
        self.token(deadline)
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
        let token = self.token(deadline)?;
        let response = github_headers(request, &token, ProviderError::AuthorizationRevoked)?
            .timeout(deadline.remaining()?)
            .send()
            .map_err(|error| map_error(&error))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(map_status(status));
        }
        decode_body(response)
    }

    fn token(&self, deadline: OperationDeadline) -> Result<SecretString, ProviderError> {
        let mut minted = self.minted.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(current) = minted.as_ref()
            && current.minted_at.elapsed() < TOKEN_REUSE
        {
            return Ok(current.token.clone());
        }
        let token = self.mint(deadline)?;
        *minted = Some(MintedToken {
            token: token.clone(),
            minted_at: Instant::now(),
        });
        Ok(token)
    }

    fn mint(&self, deadline: OperationDeadline) -> Result<SecretString, ProviderError> {
        let jwt = app_jwt(&self.app)?;
        let route = format!(
            "/app/installations/{}/access_tokens",
            self.app.installation_id
        );
        let request = self.client.post(self.url(&route)?);
        let response = github_headers(request, &jwt, ProviderError::Authentication)?
            .timeout(deadline.remaining()?)
            .send()
            .map_err(|error| map_error(&error))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(mint_status(status));
        }
        let minted: InstallationToken = decode_body(response)?;
        Ok(SecretString::from(minted.token))
    }

    fn url(&self, route: &str) -> Result<Url, ProviderError> {
        if !route.starts_with('/') || route.starts_with("//") {
            return Err(ProviderError::InvalidResponse);
        }
        Url::parse(&format!("{}{route}", self.api_base))
            .map_err(|_defect| ProviderError::InvalidResponse)
    }
}

#[derive(Serialize)]
struct AppClaims {
    iat: u64,
    exp: u64,
    iss: String,
}

#[derive(Deserialize)]
struct InstallationToken {
    token: String,
}

fn app_jwt(app: &AppCredential) -> Result<SecretString, ProviderError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_defect| ProviderError::Authentication)?
        .as_secs();
    let claims = AppClaims {
        iat: now.saturating_sub(JWT_BACKDATE_SECONDS),
        exp: now
            .checked_add(JWT_LIFETIME_SECONDS)
            .ok_or(ProviderError::Authentication)?,
        iss: app.app_id.to_string(),
    };
    let jwt = jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &app.key)
        .map_err(|_defect| ProviderError::Authentication)?;
    Ok(SecretString::from(jwt))
}

fn github_headers(
    request: RequestBuilder,
    bearer: &SecretString,
    invalid: ProviderError,
) -> Result<RequestBuilder, ProviderError> {
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", bearer.expose_secret()))
        .map_err(|_defect| invalid)?;
    authorization.set_sensitive(true);
    Ok(request
        .header(ACCEPT, GITHUB_JSON)
        .header(
            HeaderName::from_static("x-github-api-version"),
            GITHUB_API_VERSION,
        )
        .header(AUTHORIZATION, authorization))
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

fn validate_api_base(raw: &str, provider_instance: &str) -> Result<String, GitHubClientError> {
    let url = Url::parse(raw).map_err(|_defect| GitHubClientError::Configuration)?;
    let explicit_port = raw.split_once("://").is_none_or(|(_scheme, rest)| {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        authority.rsplit('@').next().unwrap_or("").contains(':')
    });
    let expected_host = if provider_instance == "github.com" {
        "api.github.com"
    } else {
        provider_instance
    };
    let valid = !raw.is_empty()
        && raw.len() <= MAX_API_BASE_BYTES
        && url.scheme() == "https"
        && url.host_str() == Some(expected_host)
        && !explicit_port
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    valid
        .then(|| raw.trim_end_matches('/').to_owned())
        .ok_or(GitHubClientError::Configuration)
}

fn map_error(error: &reqwest::Error) -> ProviderError {
    if let Some(status) = error.status() {
        return map_status(status.as_u16());
    }
    if error.is_builder() || error.is_decode() {
        ProviderError::InvalidResponse
    } else {
        ProviderError::Unavailable
    }
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

fn mint_status(status: u16) -> ProviderError {
    let mapped = map_status(status);
    if mapped == ProviderError::AuthorizationRevoked {
        ProviderError::Authentication
    } else {
        mapped
    }
}
