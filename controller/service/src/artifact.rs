use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;

use amiss_controller::{
    ArtifactComponent, ArtifactError, ArtifactStoreConfig, ControllerClock, FileArtifactStore,
    artifact_route,
};
use amiss_wire::report::MACHINE_JSON_BYTES;
use axum::Router;
use axum::body::Body;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse as _, Response};
use axum::routing::get;
use hmac::{Hmac, KeyInit as _, Mac as _};
use serde::Deserialize;
use sha2::Sha256;
use tokio::sync::Semaphore;

use crate::{ArtifactLimits, ConfigError, EndpointConfig, read_regular};

const TOKEN_BYTES: u64 = 256;
const IN_FLIGHT_ARTIFACT_BYTES: u64 = 128 * 1_024 * 1_024;
const AUTH_DOMAIN: &[u8] = b"amiss/controller-artifact-bearer-v1";

type HmacSha256 = Hmac<Sha256>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFiles {
    base_url: String,
    bearer_token_file: PathBuf,
}

pub struct ArtifactServiceConfig {
    pub root: PathBuf,
    pub store: ArtifactStoreConfig,
    route: String,
    authorization: Authorization,
    max_headers: u64,
    max_header_bytes: u64,
    max_concurrent_requests: usize,
}

pub struct ArtifactService {
    pub store: Arc<FileArtifactStore>,
    endpoint: ArtifactEndpoint,
}

#[derive(Clone)]
struct ArtifactEndpoint {
    store: Arc<FileArtifactStore>,
    route: String,
    authorization: Authorization,
    max_headers: u64,
    max_header_bytes: u64,
    permits: Arc<Semaphore>,
}

#[derive(Clone)]
struct Authorization([u8; 32]);

/// Loads one artifact route, bearer verifier, root, and set of hard limits.
///
/// # Errors
///
/// The public URL, token, root, or endpoint bounds are invalid.
pub fn load_artifact_service(
    files: &ArtifactFiles,
    root: PathBuf,
    limits: ArtifactLimits,
    endpoint: &EndpointConfig,
) -> Result<ArtifactServiceConfig, ConfigError> {
    let route = artifact_route(&files.base_url)
        .ok_or(ConfigError::invalid("artifact base URL is invalid"))?;
    let token = read_regular(&files.bearer_token_file, TOKEN_BYTES)?;
    if !(32..=usize::try_from(TOKEN_BYTES).unwrap_or(usize::MAX)).contains(&token.len())
        || !token.iter().all(|byte| bearer_byte(*byte))
    {
        return Err(ConfigError::invalid("artifact bearer token is invalid"));
    }
    let authorization = Authorization(token_mac(&token)?);
    let component_bytes = limits.record_bytes.min(MACHINE_JSON_BYTES);
    if component_bytes == 0 || endpoint.max_concurrent_requests == 0 {
        return Err(ConfigError::invalid("artifact limits are invalid"));
    }
    let memory_permits = IN_FLIGHT_ARTIFACT_BYTES
        .checked_div(component_bytes)
        .unwrap_or_default()
        .max(1);
    Ok(ArtifactServiceConfig {
        root,
        store: ArtifactStoreConfig {
            base_url: files.base_url.clone(),
            retention: limits.retention,
            max_records: limits.records,
            max_bytes: limits.bytes,
            max_record_bytes: limits.record_bytes,
        },
        route,
        authorization,
        max_headers: endpoint.max_headers,
        max_header_bytes: endpoint.max_header_bytes,
        max_concurrent_requests: endpoint
            .max_concurrent_requests
            .min(usize::try_from(memory_permits).unwrap_or(endpoint.max_concurrent_requests)),
    })
}

/// Opens the artifact root without exposing its bearer bytes to the runtime.
///
/// # Errors
///
/// The root is already owned, inconsistent, full, or corrupt.
pub fn open_artifact_service(
    config: ArtifactServiceConfig,
    clock: Arc<dyn ControllerClock>,
) -> Result<ArtifactService, ArtifactError> {
    let store = Arc::new(FileArtifactStore::open_with_clock(
        &config.root,
        config.store,
        clock,
    )?);
    let endpoint = ArtifactEndpoint {
        store: Arc::clone(&store),
        route: config.route,
        authorization: config.authorization,
        max_headers: config.max_headers,
        max_header_bytes: config.max_header_bytes,
        permits: Arc::new(Semaphore::new(config.max_concurrent_requests)),
    };
    Ok(ArtifactService { store, endpoint })
}

pub fn artifact_routes<S>(router: Router<S>, service: &ArtifactService) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let route = format!("{}/{{id}}/{{component}}", service.endpoint.route);
    router.route(
        &route,
        get(read).layer::<_, Infallible>(Extension(service.endpoint.clone())),
    )
}

async fn read(
    Extension(endpoint): Extension<ArtifactEndpoint>,
    uri: Uri,
    Path((id, component)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if uri.query().is_some() || !bounded_headers(&headers, &endpoint) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if !authorized(&headers, &endpoint.authorization) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response();
    }
    let Ok(_permit) = Arc::clone(&endpoint.permits).try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(component) = component.parse::<ArtifactComponent>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let store = Arc::clone(&endpoint.store);
    match tokio::task::spawn_blocking(move || store.read(&id, component)).await {
        Ok(Ok(bytes)) => artifact_response(bytes),
        Ok(Err(ArtifactError::NotFound)) => StatusCode::NOT_FOUND.into_response(),
        Ok(Err(
            ArtifactError::AlreadyOpen
            | ArtifactError::Configuration
            | ArtifactError::Corrupt
            | ArtifactError::Full
            | ArtifactError::TooLarge
            | ArtifactError::Conflict
            | ArtifactError::Clock
            | ArtifactError::Io(_),
        ))
        | Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn artifact_response(bytes: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(bytes));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn bounded_headers(headers: &HeaderMap, endpoint: &ArtifactEndpoint) -> bool {
    let mut count = 0_u64;
    let mut bytes = 0_u64;
    headers.iter().all(|(name, value)| {
        count = count.saturating_add(1);
        bytes = bytes
            .saturating_add(u64::try_from(name.as_str().len()).unwrap_or(u64::MAX))
            .saturating_add(u64::try_from(value.as_bytes().len()).unwrap_or(u64::MAX));
        count <= endpoint.max_headers && bytes <= endpoint.max_header_bytes
    })
}

fn authorized(headers: &HeaderMap, authorization: &Authorization) -> bool {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    let Ok(value) = value.to_str() else {
        return false;
    };
    let Some((scheme, token)) = value.split_once(' ') else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.len() > usize::try_from(TOKEN_BYTES).unwrap_or(usize::MAX)
    {
        return false;
    }
    let Ok(mut verifier) = HmacSha256::new_from_slice(token.as_bytes()) else {
        return false;
    };
    verifier.update(AUTH_DOMAIN);
    verifier.verify_slice(&authorization.0).is_ok()
}

fn token_mac(token: &[u8]) -> Result<[u8; 32], ConfigError> {
    let mut mac = HmacSha256::new_from_slice(token)
        .map_err(|_defect| ConfigError::invalid("artifact bearer token is invalid"))?;
    mac.update(AUTH_DOMAIN);
    Ok(mac.finalize().into_bytes().into())
}

const fn bearer_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/' | b'=')
}
