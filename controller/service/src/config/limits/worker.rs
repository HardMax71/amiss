use std::time::Duration;

use amiss_controller::{FileLedgerConfig, ReplayWindow};
use amiss_controller_git::GitFetchBounds;
use amiss_wire::controls::STATEMENT_TTL_MAX_SECONDS;
use amiss_wire::report::WATCHDOG_MILLISECONDS;

use super::{
    ExecutionLimits, GitLimits, HttpLimits, LedgerLimits, RunnerLimits, ServiceLimits, WorkerLimits,
};
use crate::ConfigError;

const MAX_IDLE_POLL: Duration = Duration::from_secs(5);
const MAX_LEDGER_RECORDS: u64 = 100_000;

pub(super) fn checked_execution(
    raw: &ExecutionLimits,
    replay: ReplayWindow,
    http: HttpLimits,
) -> Result<(LedgerLimits, GitLimits, RunnerLimits), ConfigError> {
    let ledger = LedgerLimits {
        lease: Duration::from_secs(raw.ledger_lease_seconds),
        records: raw.ledger_records,
    };
    let git = GitLimits {
        request: Duration::from_secs(raw.git_request_seconds),
    };
    let runner = RunnerLimits {
        bootstrap: Duration::from_secs(raw.bootstrap_seconds),
        statement_validity: Duration::from_secs(raw.statement_validity_seconds),
    };
    let statement_max = u64::try_from(STATEMENT_TTL_MAX_SECONDS).unwrap_or(u64::MAX);
    let valid = http.request < ledger.lease
        && ledger.records <= MAX_LEDGER_RECORDS
        && FileLedgerConfig::new(ledger.lease, ledger.records, replay).is_some()
        && GitFetchBounds::new(git.request).is_some()
        && !runner.bootstrap.is_zero()
        && runner.bootstrap <= Duration::from_millis(WATCHDOG_MILLISECONDS)
        && raw.statement_validity_seconds > 0
        && raw.statement_validity_seconds <= statement_max;
    valid
        .then_some((ledger, git, runner))
        .ok_or(ConfigError("runner or execution limits are invalid"))
}

pub(super) fn checked_queue(raw: &ServiceLimits) -> Result<WorkerLimits, ConfigError> {
    let queue = &raw.queue;
    let worker = WorkerLimits {
        retry_min: Duration::from_millis(queue.retry_min_millis),
        retry_max: Duration::from_millis(queue.retry_max_millis),
        idle_poll: Duration::from_millis(queue.idle_poll_millis),
    };
    let valid = !worker.retry_min.is_zero()
        && worker.retry_min <= worker.retry_max
        && !worker.idle_poll.is_zero()
        && worker.idle_poll <= MAX_IDLE_POLL;
    valid
        .then_some(worker)
        .ok_or(ConfigError("delivery worker limits are invalid"))
}
