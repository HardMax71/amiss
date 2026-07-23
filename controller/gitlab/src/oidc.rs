mod claims;
mod config;

use std::collections::BTreeMap;
use std::fmt;

use amiss_controller::{
    IngressCheck, ProviderError, ProviderIdentity, ReplayIdentity, SignedRequestProof,
    SignedTimePolicy, TrustAnchorId, TrustSetId, VerifiedDelivery,
};
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};

use self::claims::{Claims, authenticated_facts};
pub use self::config::{
    GitLabConfigError, OidcPublicKey, PolicyBinding, RunnerTrust, public_keys_from_jwks,
};
use self::config::{MAX_KEYS, validate_config};

const MAX_TOKEN_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct GitLabOidc {
    pub provider: ProviderIdentity,
    pub trust_set: TrustSetId,
    pub issuer: String,
    pub audience: String,
    pub policy: PolicyBinding,
    keys: BTreeMap<String, OidcPublicKey>,
    validation: Validation,
}

impl fmt::Debug for GitLabOidc {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitLabOidc")
            .field("provider", &self.provider)
            .field("trust_set", &self.trust_set)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("policy", &self.policy)
            .field("key_ids", &self.keys.keys())
            .finish_non_exhaustive()
    }
}

impl GitLabOidc {
    /// Creates one immutable GitLab policy-job trust set.
    ///
    /// # Errors
    ///
    /// Provider, issuer, policy, runner, or public-key bindings are invalid.
    pub fn new(
        provider: ProviderIdentity,
        trust_set: TrustSetId,
        issuer: String,
        audience: String,
        policy: PolicyBinding,
        keys: Vec<OidcPublicKey>,
        clock_skew_seconds: u64,
    ) -> Result<Self, GitLabConfigError> {
        validate_config(&provider, &issuer, &audience, &policy, &keys)?;
        let keys = keys
            .into_iter()
            .map(|key| (key.kid.clone(), key))
            .collect::<BTreeMap<_, _>>();
        if keys.is_empty() || keys.len() > MAX_KEYS {
            return Err(GitLabConfigError::invalid());
        }
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(std::slice::from_ref(&audience));
        validation.set_issuer(std::slice::from_ref(&issuer));
        validation.set_required_spec_claims(&["exp", "nbf", "aud", "iss", "sub"]);
        validation.validate_nbf = true;
        validation.leeway = clock_skew_seconds;
        validation.reject_tokens_expiring_in_less_than = 1;
        Ok(Self {
            provider,
            trust_set,
            issuer,
            audience,
            policy,
            keys,
            validation,
        })
    }

    /// Authenticates a synchronous GitLab policy-job request.
    ///
    /// # Errors
    ///
    /// Route, bearer token, signature, claims, policy origin, runner, or MR hint is invalid.
    pub fn authenticate(&self, check: IngressCheck<'_>) -> Result<VerifiedDelivery, ProviderError> {
        let input = check.delivery();
        if input.route.provider != self.provider
            || input.route.trust_set != self.trust_set
            || !matches!(input.route.signed_time, SignedTimePolicy::Required(_))
        {
            return Err(ProviderError::Authentication);
        }
        let token = bearer_token(input.headers).ok_or(ProviderError::Authentication)?;
        let header = decode_header(token).map_err(|_defect| ProviderError::Authentication)?;
        if header.alg != Algorithm::RS256 {
            return Err(ProviderError::Authentication);
        }
        let kid = header.kid.ok_or(ProviderError::Authentication)?;
        let key = self.keys.get(&kid).ok_or(ProviderError::Authentication)?;
        let claims = decode::<Claims>(token, &key.key, &self.validation)
            .map_err(|_defect| ProviderError::Authentication)?
            .claims;
        let facts = authenticated_facts(&self.provider, &self.policy, &claims, input.body)?;
        let proof = signed_request_proof(
            check,
            self.trust_set.clone(),
            key.anchor.clone(),
            ReplayIdentity::Authenticated(facts.replay),
            Some(facts.issued_at_unix_millis),
        );
        Ok(proof.bind(facts.delivery))
    }
}

fn bearer_token<'a>(headers: &'a [amiss_controller::DeliveryHeader<'a>]) -> Option<&'a str> {
    let mut values = headers
        .iter()
        .filter(|header| header.name.eq_ignore_ascii_case("authorization"));
    let value = values.next()?.value;
    values.next().is_none().then_some(())?;
    let raw = std::str::from_utf8(value).ok()?;
    let token = raw.strip_prefix("Bearer ")?;
    (!token.is_empty()
        && token.len() <= MAX_TOKEN_BYTES
        && !token.bytes().any(|byte| byte.is_ascii_whitespace()))
    .then_some(token)
}

fn signed_request_proof(
    check: IngressCheck<'_>,
    trust_set: TrustSetId,
    anchor: TrustAnchorId,
    replay: ReplayIdentity,
    issued_at_unix_millis: Option<i64>,
) -> SignedRequestProof {
    SignedRequestProof::verified(check, trust_set, anchor, replay, issued_at_unix_millis)
}
