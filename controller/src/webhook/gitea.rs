use crate::IngressCheck;

use super::{WebhookError, WebhookKeyring, WebhookProof, body_signature};

const SIGNATURE_HEADERS: [&str; 2] = ["x-gitea-signature", "x-forgejo-signature"];

/// Verifies Gitea-family HMAC-SHA256 signatures over the exact request body.
#[derive(Debug)]
pub struct GiteaWebhook {
    keys: WebhookKeyring,
}

impl GiteaWebhook {
    pub const fn new(keys: WebhookKeyring) -> Self {
        Self { keys }
    }

    /// Authenticates only the raw body. Gitea's delivery ID header is not
    /// covered by this signature and is deliberately not returned.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid headers, an inactive key set, or a
    /// signature mismatch.
    pub fn verify(&self, check: IngressCheck<'_>) -> Result<WebhookProof, WebhookError> {
        body_signature::verify_one_of(&self.keys, check, &SIGNATURE_HEADERS, b"")
    }
}
