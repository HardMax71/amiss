use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExecutionLimits {
    pub(super) body_bytes: usize,
    pub(super) header_count: usize,
    pub(super) header_bytes: usize,
    pub(super) queue_age_seconds: u64,
    pub(super) future_skew_seconds: u64,
    pub(super) ledger_lease_seconds: u64,
    pub(super) ledger_records: u64,
    pub(super) api_connect_millis: u64,
    pub(super) api_read_millis: u64,
    pub(super) api_write_millis: u64,
    pub(super) api_request_millis: u64,
    pub(super) git_request_seconds: u64,
    pub(super) bootstrap_seconds: u64,
    pub(super) statement_validity_seconds: u64,
}

const DEFAULT_EXECUTION_LIMITS: ExecutionLimits = ExecutionLimits {
    body_bytes: 2_097_152,
    header_count: 64,
    header_bytes: 32_768,
    queue_age_seconds: 86_400,
    future_skew_seconds: 5,
    ledger_lease_seconds: 60,
    ledger_records: 50_000,
    api_connect_millis: 5_000,
    api_read_millis: 15_000,
    api_write_millis: 15_000,
    api_request_millis: 20_000,
    git_request_seconds: 120,
    bootstrap_seconds: 120,
    statement_validity_seconds: 300,
};

impl Default for ExecutionLimits {
    fn default() -> Self {
        DEFAULT_EXECUTION_LIMITS
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceLimits {
    pub(super) execution: ExecutionLimits,
    pub(super) queue: QueueLimits,
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct QueueLimits {
    pub(super) inbox_lease_seconds: u64,
    pub(super) inbox_records: u64,
    pub(super) inbox_bytes: u64,
    pub(super) inbox_record_bytes: u64,
    pub(super) retry_min_millis: u64,
    pub(super) retry_max_millis: u64,
    pub(super) idle_poll_millis: u64,
    pub(super) max_concurrent_deliveries: usize,
}

const DEFAULT_QUEUE_LIMITS: QueueLimits = QueueLimits {
    inbox_lease_seconds: 600,
    inbox_records: 64,
    inbox_bytes: 134_217_728,
    inbox_record_bytes: 3_145_728,
    retry_min_millis: 1_000,
    retry_max_millis: 60_000,
    idle_poll_millis: 250,
    max_concurrent_deliveries: 16,
};

impl Default for QueueLimits {
    fn default() -> Self {
        DEFAULT_QUEUE_LIMITS
    }
}
