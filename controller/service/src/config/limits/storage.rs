use std::time::Duration;

use crate::limits::{MAX_INBOX_BYTES, MAX_INBOX_RECORD_BYTES, MAX_INBOX_RECORDS};
use crate::{ConfigError, InboxLimits};

use super::ServiceLimits;

pub(super) fn checked_inbox(raw: &ServiceLimits) -> Result<InboxLimits, ConfigError> {
    let body_bytes = u64::try_from(raw.execution.body_bytes)
        .map_err(|_defect| ConfigError("body limit is too large"))?;
    let header_count = u64::try_from(raw.execution.header_count)
        .map_err(|_defect| ConfigError("header count is too large"))?;
    let header_bytes = u64::try_from(raw.execution.header_bytes)
        .map_err(|_defect| ConfigError("header byte limit is too large"))?;
    inbox_limits(raw, body_bytes, header_count, header_bytes)
}

fn inbox_limits(
    raw: &ServiceLimits,
    body_bytes: u64,
    header_count: u64,
    header_bytes: u64,
) -> Result<InboxLimits, ConfigError> {
    let record_minimum = base64_size(body_bytes)
        .and_then(|bytes| bytes.checked_add(base64_size(header_bytes)?))
        .and_then(|bytes| bytes.checked_add(header_count.checked_mul(256)?.checked_add(65_536)?))
        .ok_or(ConfigError("inbox limits overflow"))?;
    let queue = &raw.queue;
    let lease = Duration::from_secs(queue.inbox_lease_seconds);
    let shape = queue.inbox_lease_seconds > 0
        && (1..=MAX_INBOX_RECORDS).contains(&queue.inbox_records)
        && (1..=MAX_INBOX_BYTES).contains(&queue.inbox_bytes)
        && (1..=MAX_INBOX_RECORD_BYTES).contains(&queue.inbox_record_bytes)
        && queue.inbox_record_bytes >= record_minimum
        && queue
            .inbox_record_bytes
            .checked_mul(2)
            .is_some_and(|minimum| minimum <= queue.inbox_bytes)
        && i64::try_from(lease.as_millis()).is_ok_and(|millis| millis > 0);
    if !shape {
        return Err(ConfigError("inbox limits cannot hold one bounded request"));
    }
    Ok(InboxLimits {
        lease_duration: lease,
        max_records: queue.inbox_records,
        max_bytes: queue.inbox_bytes,
        max_record_bytes: queue.inbox_record_bytes,
        max_body_bytes: body_bytes,
        max_headers: header_count,
        max_header_bytes: header_bytes,
        max_route_bytes: 128,
        max_source_id_bytes: 128,
    })
}

fn base64_size(bytes: u64) -> Option<u64> {
    bytes.checked_add(2)?.checked_div(3)?.checked_mul(4)
}
