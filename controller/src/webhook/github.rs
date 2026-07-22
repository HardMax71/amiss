use crate::IngressCheck;

use super::{WebhookError, WebhookKeyring, WebhookProof, body_signature};

const SIGNATURE_HEADER: &str = "x-hub-signature-256";

/// Verifies GitHub's HMAC-SHA256 signature over the exact request body.
#[derive(Debug)]
pub struct GitHubWebhook {
    keys: WebhookKeyring,
}

impl GitHubWebhook {
    pub const fn new(keys: WebhookKeyring) -> Self {
        Self { keys }
    }

    /// Authenticates only the raw body. GitHub's delivery ID header is not
    /// covered by this signature and is deliberately not returned.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid headers, an inactive key set, or a
    /// signature mismatch.
    pub fn verify(&self, check: IngressCheck<'_>) -> Result<WebhookProof, WebhookError> {
        body_signature::verify(&self.keys, check, SIGNATURE_HEADER, b"sha256=")
    }
}
