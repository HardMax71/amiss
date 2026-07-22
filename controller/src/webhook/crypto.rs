use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;

use super::WebhookError;

type HmacSha256 = Hmac<Sha256>;

pub(super) fn lowercase_hex(raw: &[u8]) -> Result<[u8; 32], WebhookError> {
    if raw.len() != 64
        || !raw
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(WebhookError::Headers);
    }
    let mut signature = [0_u8; 32];
    hex::decode_to_slice(raw, &mut signature).map_err(|_| WebhookError::Headers)?;
    Ok(signature)
}

pub(super) fn verify(secret: &[u8], signature: &[u8; 32], message_parts: &[&[u8]]) -> bool {
    let Ok(mut mac) = HmacSha256::new_from_slice(secret) else {
        return false;
    };
    for part in message_parts {
        mac.update(part);
    }
    mac.verify_slice(signature).is_ok()
}
