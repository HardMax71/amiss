use std::path::PathBuf;

use amiss_controller::{TrustAnchorId, TrustSetId, WebhookKey, WebhookKeyring};
use serde::Deserialize;

use super::{ConfigError, read_regular};

const WEBHOOK_SECRET_BYTES: u64 = 1_024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookKeyFile {
    id: String,
    secret_file: PathBuf,
    active_from_unix_millis: i64,
    active_until_unix_millis: Option<i64>,
}

/// Loads one rotating provider webhook keyring from bounded secret files.
///
/// # Errors
///
/// A key identity, secret file, activation window, or keyring is invalid.
pub fn load_webhook_keyring(
    trust_set: TrustSetId,
    raw: Vec<WebhookKeyFile>,
) -> Result<WebhookKeyring, ConfigError> {
    let keys = raw
        .into_iter()
        .map(|key| {
            let anchor =
                TrustAnchorId::new(key.id).ok_or(ConfigError("webhook key identity is invalid"))?;
            let secret = read_regular(&key.secret_file, WEBHOOK_SECRET_BYTES)?;
            WebhookKey::new(
                anchor,
                secret,
                key.active_from_unix_millis,
                key.active_until_unix_millis,
            )
            .map_err(|_defect| ConfigError("webhook key is invalid"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    WebhookKeyring::new(trust_set, keys)
        .map_err(|_defect| ConfigError("webhook key set is invalid"))
}
