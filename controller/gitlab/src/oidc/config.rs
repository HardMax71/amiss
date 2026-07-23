use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use amiss_controller::{IntegrationId, ProviderIdentity, TrustAnchorId};
use amiss_wire::model::Oid;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{AlgorithmFamily, DecodingKey};
use url::Url;

use crate::identity::{canonical_project_path, exact_sha1};

pub(crate) const MAX_KEYS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerTrust {
    pub gitlab_hosted: bool,
    pub self_hosted_ids: BTreeSet<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyBinding {
    pub integration: IntegrationId,
    pub project_id: u64,
    pub project_path: String,
    pub target_branch: String,
    pub job_name: String,
    pub config_url: String,
    pub config_commit: Oid,
    pub runners: RunnerTrust,
}

#[derive(Clone, Debug)]
pub struct OidcPublicKey {
    pub kid: String,
    pub anchor: TrustAnchorId,
    pub(crate) key: DecodingKey,
}

impl OidcPublicKey {
    /// Builds one pinned RSA verification key.
    ///
    /// # Errors
    ///
    /// The key identifier or PEM public key is invalid.
    pub fn from_rsa_pem(
        kid: String,
        anchor: TrustAnchorId,
        pem: &[u8],
    ) -> Result<Self, GitLabConfigError> {
        valid_kid(&kid)?;
        let key = DecodingKey::from_rsa_pem(pem).map_err(|_defect| GitLabConfigError::invalid())?;
        Ok(Self { kid, anchor, key })
    }
}

/// Converts a provider JWKS into pinned RSA keys.
///
/// # Errors
///
/// The set is empty, oversized, duplicated, non-RSA, or not exactly anchored.
pub fn public_keys_from_jwks(
    jwks: &JwkSet,
    anchors: &BTreeMap<String, TrustAnchorId>,
) -> Result<Vec<OidcPublicKey>, GitLabConfigError> {
    if jwks.keys.is_empty() || jwks.keys.len() > MAX_KEYS || jwks.keys.len() != anchors.len() {
        return Err(GitLabConfigError::invalid());
    }
    let keys = jwks
        .keys
        .iter()
        .map(|jwk| {
            let kid = jwk
                .common
                .key_id
                .clone()
                .ok_or_else(GitLabConfigError::invalid)?;
            valid_kid(&kid)?;
            let anchor = anchors
                .get(&kid)
                .cloned()
                .ok_or_else(GitLabConfigError::invalid)?;
            let key = DecodingKey::from_jwk(jwk).map_err(|_defect| GitLabConfigError::invalid())?;
            (key.family() == AlgorithmFamily::Rsa)
                .then_some(OidcPublicKey { kid, anchor, key })
                .ok_or_else(GitLabConfigError::invalid)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let unique = keys
        .iter()
        .map(|key| key.kid.as_str())
        .collect::<BTreeSet<_>>();
    (unique.len() == keys.len())
        .then_some(keys)
        .ok_or_else(GitLabConfigError::invalid)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitLabConfigError;

impl GitLabConfigError {
    pub(crate) const fn invalid() -> Self {
        Self
    }
}

impl fmt::Display for GitLabConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the GitLab OIDC configuration is invalid")
    }
}

impl std::error::Error for GitLabConfigError {}

pub(crate) fn validate_config(
    provider: &ProviderIdentity,
    issuer: &str,
    audience: &str,
    policy: &PolicyBinding,
    keys: &[OidcPublicKey],
) -> Result<(), GitLabConfigError> {
    let issuer_url = Url::parse(issuer).map_err(|_defect| GitLabConfigError::invalid())?;
    let policy_url =
        Url::parse(&policy.config_url).map_err(|_defect| GitLabConfigError::invalid())?;
    let expected_path = canonical_project_path(&policy.project_path);
    let unique_keys = keys
        .iter()
        .map(|key| key.kid.as_str())
        .collect::<BTreeSet<_>>();
    let urls_valid = [issuer_url, policy_url].into_iter().all(|url| {
        url.scheme() == "https"
            && url.host_str() == Some(provider.instance.as_str())
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
    });
    let runners_valid = policy.runners.gitlab_hosted || !policy.runners.self_hosted_ids.is_empty();
    let valid = provider.namespace.as_str() == "gitlab"
        && urls_valid
        && !audience.is_empty()
        && audience.len() <= 2_048
        && policy.project_id > 0
        && expected_path.as_deref() == Some(policy.project_path.as_str())
        && !policy.target_branch.is_empty()
        && policy.target_branch.len() <= 255
        && !policy.job_name.is_empty()
        && policy.job_name.len() <= 255
        && exact_sha1(policy.config_commit.as_str()).as_ref() == Some(&policy.config_commit)
        && runners_valid
        && !keys.is_empty()
        && keys.len() <= MAX_KEYS
        && unique_keys.len() == keys.len()
        && keys.iter().all(|key| valid_kid(&key.kid).is_ok());
    valid.then_some(()).ok_or_else(GitLabConfigError::invalid)
}

fn valid_kid(kid: &str) -> Result<(), GitLabConfigError> {
    (!kid.is_empty()
        && kid.len() <= 256
        && kid
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\')))
    .then_some(())
    .ok_or_else(GitLabConfigError::invalid)
}
