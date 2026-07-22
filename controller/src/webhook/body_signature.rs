use crate::{IngressCheck, ReplayIdentity};

use super::headers::Headers;
use super::{WebhookError, WebhookKeyring, WebhookProof, crypto};

pub(super) fn verify(
    keys: &WebhookKeyring,
    check: IngressCheck<'_>,
    header_name: &str,
    prefix: &[u8],
) -> Result<WebhookProof, WebhookError> {
    let delivery = check.delivery();
    let headers = Headers::new(delivery.headers)?;
    let raw = headers.exact(header_name, prefix.len().saturating_add(64))?;
    let encoded = raw.strip_prefix(prefix).ok_or(WebhookError::Headers)?;
    let signature = crypto::lowercase_hex(encoded)?;
    let anchor = keys.authenticate(
        delivery.received_at_unix_millis,
        &[signature],
        &[delivery.body],
    )?;
    Ok(WebhookProof::new(
        check,
        keys.trust_set().clone(),
        anchor,
        ReplayIdentity::ExactBody,
        None,
    ))
}
