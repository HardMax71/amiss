use std::time::Duration;

use super::{ExecutionLimits, HttpLimits};
use crate::ConfigError;

const MAX_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn checked_http(raw: &ExecutionLimits) -> Result<HttpLimits, ConfigError> {
    let limits = HttpLimits {
        connect: Duration::from_millis(raw.api_connect_millis),
        read: Duration::from_millis(raw.api_read_millis),
        write: Duration::from_millis(raw.api_write_millis),
        request: Duration::from_millis(raw.api_request_millis),
    };
    let phases = [limits.connect, limits.read, limits.write];
    let valid = phases
        .into_iter()
        .all(|timeout| !timeout.is_zero() && timeout <= MAX_HTTP_TIMEOUT)
        && !limits.request.is_zero()
        && limits.request <= MAX_HTTP_TIMEOUT
        && phases.into_iter().all(|timeout| timeout <= limits.request);
    valid
        .then_some(limits)
        .ok_or(ConfigError("HTTP timeouts are invalid"))
}
