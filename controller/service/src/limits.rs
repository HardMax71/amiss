use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::InboxError;

pub(crate) const MAX_INBOX_RECORDS: u64 = 1_024;
pub(crate) const MAX_INBOX_BYTES: u64 = 128 * 1_024 * 1_024;
pub(crate) const MAX_INBOX_RECORD_BYTES: u64 = 16 * 1_024 * 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InboxLimits {
    pub lease_duration: Duration,
    pub max_records: u64,
    pub max_bytes: u64,
    pub max_record_bytes: u64,
    pub max_body_bytes: u64,
    pub max_headers: u64,
    pub max_header_bytes: u64,
    pub max_route_bytes: u64,
    pub max_source_id_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredLimits {
    lease_millis: i64,
    max_records: u64,
    max_bytes: u64,
    max_record_bytes: u64,
    max_body_bytes: u64,
    max_headers: u64,
    max_header_bytes: u64,
    max_route_bytes: u64,
    max_source_id_bytes: u64,
}

impl StoredLimits {
    pub(crate) fn read(limits: InboxLimits) -> Result<Self, InboxError> {
        let lease_millis = i64::try_from(limits.lease_duration.as_millis())
            .ok()
            .filter(|millis| *millis > 0)
            .ok_or(InboxError::Configuration)?;
        if !(1..=MAX_INBOX_RECORDS).contains(&limits.max_records)
            || !(1..=MAX_INBOX_BYTES).contains(&limits.max_bytes)
            || !(1..=MAX_INBOX_RECORD_BYTES).contains(&limits.max_record_bytes)
            || [
                limits.max_body_bytes,
                limits.max_headers,
                limits.max_header_bytes,
                limits.max_route_bytes,
                limits.max_source_id_bytes,
            ]
            .contains(&0)
            || limits
                .max_record_bytes
                .checked_mul(2)
                .is_none_or(|minimum| minimum > limits.max_bytes)
        {
            return Err(InboxError::Configuration);
        }
        Ok(Self {
            lease_millis,
            max_records: limits.max_records,
            max_bytes: limits.max_bytes,
            max_record_bytes: limits.max_record_bytes,
            max_body_bytes: limits.max_body_bytes,
            max_headers: limits.max_headers,
            max_header_bytes: limits.max_header_bytes,
            max_route_bytes: limits.max_route_bytes,
            max_source_id_bytes: limits.max_source_id_bytes,
        })
    }

    pub(crate) const fn lease_millis(self) -> i64 {
        self.lease_millis
    }

    pub(crate) const fn max_records(self) -> u64 {
        self.max_records
    }

    pub(crate) const fn max_bytes(self) -> u64 {
        self.max_bytes
    }

    pub(crate) const fn max_record_bytes(self) -> u64 {
        self.max_record_bytes
    }

    pub(crate) const fn max_body_bytes(self) -> u64 {
        self.max_body_bytes
    }

    pub(crate) const fn max_headers(self) -> u64 {
        self.max_headers
    }

    pub(crate) const fn max_header_bytes(self) -> u64 {
        self.max_header_bytes
    }

    pub(crate) const fn max_route_bytes(self) -> u64 {
        self.max_route_bytes
    }

    pub(crate) const fn max_source_id_bytes(self) -> u64 {
        self.max_source_id_bytes
    }
}
