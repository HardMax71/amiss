use crate::{IngressCheck, ReplayIdentity};

use super::headers::Headers;
use super::{WebhookError, WebhookKeyring, WebhookProof, crypto};

pub(super) fn verify(
    keys: &WebhookKeyring,
    check: IngressCheck<'_>,
    header_name: &str,
    prefix: &[u8],
) -> Result<WebhookProof, WebhookError> {
    verify_one_of(keys, check, &[header_name], prefix)
}

pub(super) fn verify_one_of(
    keys: &WebhookKeyring,
    check: IngressCheck<'_>,
    header_names: &[&str],
    prefix: &[u8],
) -> Result<WebhookProof, WebhookError> {
    let delivery = check.delivery();
    let headers = Headers::new(delivery.headers)?;
    let raw = headers.one_of(header_names, prefix.len().saturating_add(64))?;
    authenticate(keys, check, raw, prefix)
}

fn authenticate(
    keys: &WebhookKeyring,
    check: IngressCheck<'_>,
    raw: &[u8],
    prefix: &[u8],
) -> Result<WebhookProof, WebhookError> {
    let delivery = check.delivery();
    let encoded = raw.strip_prefix(prefix).ok_or(WebhookError::Headers)?;
    let signature = crypto::lowercase_hex(encoded)?;
    let anchor = keys.authenticate(
        delivery.received_at_unix_millis,
        &[signature],
        &[delivery.body],
    )?;
    Ok(WebhookProof::verified(
        check,
        keys.trust_set().clone(),
        anchor,
        ReplayIdentity::ExactBody,
        None,
    ))
}
