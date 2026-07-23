use base64::Engine as _;

use crate::{DeliveryId, IngressCheck, ReplayIdentity};

use super::headers::Headers;
use super::{WebhookError, WebhookKeyring, WebhookProof};

const ID_HEADER: &str = "webhook-id";
const TIMESTAMP_HEADER: &str = "webhook-timestamp";
const SIGNATURE_HEADER: &str = "webhook-signature";
const MAX_ID_BYTES: usize = 256;
const MAX_TIMESTAMP_BYTES: usize = 19;
const MAX_SIGNATURES: usize = 8;
const ENCODED_SIGNATURE_BYTES: usize = 47;
const MAX_SIGNATURE_HEADER_BYTES: usize = MAX_SIGNATURES * (ENCODED_SIGNATURE_BYTES + 1) - 1;

/// Verifies GitLab 19.1+ Standard Webhooks signing tokens.
///
/// Legacy `X-Gitlab-Token` values are intentionally unsupported because they
/// do not authenticate the body, delivery ID, or request time.
#[derive(Debug)]
pub struct GitLabWebhook {
    keys: WebhookKeyring,
}

impl GitLabWebhook {
    pub const fn new(keys: WebhookKeyring) -> Self {
        Self { keys }
    }

    /// Authenticates the Standard Webhooks ID, timestamp, and exact body.
    /// Freshness is enforced later by controller ingress policy.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid headers, an inactive key set, or a
    /// signature mismatch.
    pub fn verify(&self, check: IngressCheck<'_>) -> Result<WebhookProof, WebhookError> {
        let request = check.delivery();
        let headers = Headers::new(request.headers)?;
        let raw_id = headers.exact(ID_HEADER, MAX_ID_BYTES)?;
        let raw_timestamp = headers.exact(TIMESTAMP_HEADER, MAX_TIMESTAMP_BYTES)?;
        let raw_signatures = headers.exact(SIGNATURE_HEADER, MAX_SIGNATURE_HEADER_BYTES)?;
        let delivery_id = delivery_id(raw_id)?;
        let issued_at_unix_millis = timestamp_millis(raw_timestamp)?;
        let signatures = signatures(raw_signatures)?;
        let anchor = self.keys.authenticate(
            request.received_at_unix_millis,
            &signatures,
            &[raw_id, b".", raw_timestamp, b".", request.body],
        )?;
        Ok(WebhookProof::verified(
            check,
            self.keys.trust_set().clone(),
            anchor,
            ReplayIdentity::Authenticated(delivery_id),
            Some(issued_at_unix_millis),
        ))
    }
}

fn delivery_id(raw: &[u8]) -> Result<DeliveryId, WebhookError> {
    if raw.contains(&b'.') {
        return Err(WebhookError::Headers);
    }
    let value = std::str::from_utf8(raw).map_err(|_| WebhookError::Headers)?;
    DeliveryId::new(value.to_owned()).ok_or(WebhookError::Headers)
}

fn timestamp_millis(raw: &[u8]) -> Result<i64, WebhookError> {
    if !raw.iter().all(u8::is_ascii_digit) || (raw.len() > 1 && raw.first() == Some(&b'0')) {
        return Err(WebhookError::Headers);
    }
    let seconds = raw.iter().try_fold(0_i64, |value, byte| {
        value
            .checked_mul(10)?
            .checked_add(i64::from(*byte) - i64::from(b'0'))
    });
    seconds
        .and_then(|seconds| seconds.checked_mul(1_000))
        .ok_or(WebhookError::Headers)
}

fn signatures(raw: &[u8]) -> Result<Vec<[u8; 32]>, WebhookError> {
    let mut decoded = Vec::new();
    for encoded in raw.split(|byte| *byte == b' ') {
        if decoded.len() >= MAX_SIGNATURES || encoded.len() != ENCODED_SIGNATURE_BYTES {
            return Err(WebhookError::Headers);
        }
        let encoded = encoded.strip_prefix(b"v1,").ok_or(WebhookError::Headers)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| WebhookError::Headers)?;
        let signature: [u8; 32] = bytes.try_into().map_err(|_| WebhookError::Headers)?;
        if decoded.contains(&signature) {
            return Err(WebhookError::Headers);
        }
        decoded.push(signature);
    }
    if decoded.is_empty() {
        return Err(WebhookError::Headers);
    }
    Ok(decoded)
}
