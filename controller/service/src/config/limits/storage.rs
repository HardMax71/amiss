use std::time::Duration;

use crate::limits::StoredLimits;
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
    let queue = &raw.queue;
    let limits = InboxLimits {
        lease_duration: Duration::from_secs(queue.inbox_lease_seconds),
        max_records: queue.inbox_records,
        max_bytes: queue.inbox_bytes,
        max_record_bytes: queue.inbox_record_bytes,
        max_body_bytes: body_bytes,
        max_headers: header_count,
        max_header_bytes: header_bytes,
        max_route_bytes: 128,
        max_source_id_bytes: 128,
    };
    StoredLimits::read(limits)
        .map(|_stored| limits)
        .map_err(|_defect| ConfigError("inbox limits cannot hold one bounded request"))
}
