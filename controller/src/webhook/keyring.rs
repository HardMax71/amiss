use std::fmt;

use base64::Engine as _;
use secrecy::{ExposeSecret as _, SecretSlice, SecretString};

use crate::{TrustAnchorId, TrustSetId};

use super::{WebhookError, WebhookKeyringError, crypto};

const MAX_KEYS: usize = 8;
const MIN_SECRET_BYTES: usize = 16;
const MAX_SECRET_BYTES: usize = 1_024;
const MIN_STANDARD_SECRET_BYTES: usize = 24;
const MAX_STANDARD_SECRET_BYTES: usize = 64;
const MAX_STANDARD_ENCODED_BYTES: usize = 88;

/// One HMAC key and its controller-owned acceptance window.
pub struct WebhookKey {
    anchor: TrustAnchorId,
    secret: SecretSlice<u8>,
    active_from_unix_millis: i64,
    active_until_unix_millis: Option<i64>,
}

impl WebhookKey {
    /// Builds a key from raw HMAC bytes, immediately moving them into
    /// zeroizing, redacted storage.
    ///
    /// # Errors
    ///
    /// Returns an error for a weak or oversized secret or an invalid window.
    pub fn new(
        anchor: TrustAnchorId,
        secret: Vec<u8>,
        active_from_unix_millis: i64,
        active_until_unix_millis: Option<i64>,
    ) -> Result<Self, WebhookKeyringError> {
        Self::from_secret(
            anchor,
            SecretSlice::from(secret),
            active_from_unix_millis,
            active_until_unix_millis,
        )
    }

    fn from_secret(
        anchor: TrustAnchorId,
        secret: SecretSlice<u8>,
        active_from_unix_millis: i64,
        active_until_unix_millis: Option<i64>,
    ) -> Result<Self, WebhookKeyringError> {
        if !(MIN_SECRET_BYTES..=MAX_SECRET_BYTES).contains(&secret.expose_secret().len()) {
            return Err(WebhookKeyringError::Secret);
        }
        if active_from_unix_millis < 0
            || active_until_unix_millis.is_some_and(|until| until <= active_from_unix_millis)
        {
            return Err(WebhookKeyringError::Window);
        }
        Ok(Self {
            anchor,
            secret,
            active_from_unix_millis,
            active_until_unix_millis,
        })
    }

    /// Builds a key from a GitLab/Standard Webhooks `whsec_` token without
    /// exposing its text through this API or debug output.
    ///
    /// # Errors
    ///
    /// Returns an error unless the prefix and canonical padded Base64 are
    /// exact and the decoded Standard Webhooks key is 24 through 64 bytes.
    pub fn from_standard_token(
        anchor: TrustAnchorId,
        token: SecretString,
        active_from_unix_millis: i64,
        active_until_unix_millis: Option<i64>,
    ) -> Result<Self, WebhookKeyringError> {
        let encoded = token
            .expose_secret()
            .strip_prefix("whsec_")
            .filter(|encoded| !encoded.is_empty() && encoded.len() <= MAX_STANDARD_ENCODED_BYTES)
            .ok_or(WebhookKeyringError::Secret)?;
        let secret = SecretSlice::from(
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| WebhookKeyringError::Secret)?,
        );
        drop(token);
        if !(MIN_STANDARD_SECRET_BYTES..=MAX_STANDARD_SECRET_BYTES)
            .contains(&secret.expose_secret().len())
        {
            return Err(WebhookKeyringError::Secret);
        }
        Self::from_secret(
            anchor,
            secret,
            active_from_unix_millis,
            active_until_unix_millis,
        )
    }

    fn is_active(&self, received_at_unix_millis: i64) -> bool {
        received_at_unix_millis >= self.active_from_unix_millis
            && self
                .active_until_unix_millis
                .is_none_or(|until| received_at_unix_millis < until)
    }
}

impl fmt::Debug for WebhookKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        format_webhook_key(self, formatter)
    }
}

fn format_webhook_key(key: &WebhookKey, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
        .debug_struct("WebhookKey")
        .field("anchor", &key.anchor)
        .field("secret", &"[REDACTED]")
        .field("active_from_unix_millis", &key.active_from_unix_millis)
        .field("active_until_unix_millis", &key.active_until_unix_millis)
        .finish()
}

/// A bounded immutable set of current and retiring webhook keys.
///
/// Replacing this value revokes every omitted anchor. Overlapping windows
/// permit rotation; the newest matching active anchor is reported.
#[derive(Debug)]
pub struct WebhookKeyring {
    trust_set: TrustSetId,
    keys: Vec<WebhookKey>,
}

impl WebhookKeyring {
    /// Validates and orders a complete trust-anchor set.
    ///
    /// # Errors
    ///
    /// Returns an error when the set is empty, exceeds the rotation bound, or
    /// repeats an anchor ID or secret.
    pub fn new(
        trust_set: TrustSetId,
        mut keys: Vec<WebhookKey>,
    ) -> Result<Self, WebhookKeyringError> {
        if keys.is_empty() {
            return Err(WebhookKeyringError::Empty);
        }
        if keys.len() > MAX_KEYS {
            return Err(WebhookKeyringError::TooMany);
        }
        for (position, key) in keys.iter().enumerate() {
            for other in keys.iter().skip(position.saturating_add(1)) {
                if key.anchor == other.anchor {
                    return Err(WebhookKeyringError::DuplicateAnchor);
                }
                if key.secret.expose_secret() == other.secret.expose_secret() {
                    return Err(WebhookKeyringError::DuplicateSecret);
                }
            }
        }
        keys.sort_by(|left, right| {
            right
                .active_from_unix_millis
                .cmp(&left.active_from_unix_millis)
                .then_with(|| left.anchor.cmp(&right.anchor))
        });
        Ok(Self { trust_set, keys })
    }

    pub fn trust_set(&self) -> &TrustSetId {
        &self.trust_set
    }

    pub(super) fn authenticate(
        &self,
        received_at_unix_millis: i64,
        signatures: &[[u8; 32]],
        message_parts: &[&[u8]],
    ) -> Result<TrustAnchorId, WebhookError> {
        if received_at_unix_millis < 0 {
            return Err(WebhookError::ReceiptTime);
        }
        let mut active = false;
        let mut matched = None;
        for key in self
            .keys
            .iter()
            .filter(|key| key.is_active(received_at_unix_millis))
        {
            active = true;
            let mut key_matches = false;
            for signature in signatures {
                key_matches = crypto::verify(key.secret.expose_secret(), signature, message_parts)
                    || key_matches;
            }
            if key_matches && matched.is_none() {
                matched = Some(key.anchor.clone());
            }
        }
        if !active {
            return Err(WebhookError::NoActiveAnchor);
        }
        matched.ok_or(WebhookError::Authentication)
    }
}
