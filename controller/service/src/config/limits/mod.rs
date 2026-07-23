mod http;
mod model;
mod raw;
mod storage;
mod worker;

use std::time::Duration;

use amiss_controller::{IngressLimits, IngressPolicy, ReplayWindow};

use crate::evaluation::{
    MAX_BODY_BYTES, MAX_CONCURRENT_EVALUATIONS, MAX_HEADER_BYTES, MAX_HEADERS,
};
use crate::receiver::MAX_CONCURRENT_DELIVERIES;
use crate::{EvaluationConfig, ReceiverConfig};

use self::http::checked_http;
use self::model::{CommonLimits, EndpointLimits};
pub use self::model::{
    GitLimits, HttpLimits, LedgerLimits, LoadedExecutionLimits, LoadedLimits, RunnerLimits,
    WorkerLimits,
};
pub use self::raw::{ExecutionLimits, ServiceLimits};
use self::storage::checked_inbox;
use self::worker::{checked_execution, checked_queue};
use super::ConfigError;

const SIGNED_AGE: Duration = Duration::from_mins(5);
const MAX_FUTURE_SKEW: Duration = Duration::from_mins(5);

/// Validates all queued service limits and binds one webhook receiver path.
///
/// # Errors
///
/// An ingress, replay, storage, HTTP, Git, runner, worker, or receiver limit
/// is inconsistent, zero, overflowing, or outside its hard ceiling.
pub fn load_limits(raw: &ServiceLimits, webhook_path: String) -> Result<LoadedLimits, ConfigError> {
    let common = checked_common(&raw.execution)?;
    let inbox = checked_inbox(raw)?;
    let worker = checked_queue(raw)?;
    if !(1..=MAX_CONCURRENT_DELIVERIES).contains(&raw.queue.max_concurrent_deliveries) {
        return Err(ConfigError("delivery concurrency is invalid"));
    }
    let receiver = ReceiverConfig {
        delivery_path: webhook_path,
        max_body_bytes: common.endpoint.body_bytes,
        max_headers: common.endpoint.headers,
        max_header_bytes: common.endpoint.header_bytes,
        max_concurrent_deliveries: raw.queue.max_concurrent_deliveries,
    };
    Ok(LoadedLimits {
        receiver,
        inbox,
        ledger: common.ledger,
        ingress: common.ingress,
        replay: common.replay,
        signed_age: common.signed_age,
        future_skew: common.future_skew,
        http: common.http,
        git: common.git,
        runner: common.runner,
        worker,
    })
}

/// Validates execution limits and binds one synchronous evaluation endpoint.
///
/// # Errors
///
/// An ingress, replay, ledger, HTTP, Git, runner, or endpoint limit is
/// inconsistent, zero, overflowing, or outside its hard ceiling.
pub fn load_execution_limits(
    raw: &ExecutionLimits,
    evaluation_path: String,
    max_concurrent_evaluations: usize,
) -> Result<LoadedExecutionLimits, ConfigError> {
    if !(1..=MAX_CONCURRENT_EVALUATIONS).contains(&max_concurrent_evaluations) {
        return Err(ConfigError("evaluation concurrency is invalid"));
    }
    let common = checked_common(raw)?;
    Ok(LoadedExecutionLimits {
        evaluation: EvaluationConfig {
            path: evaluation_path,
            max_body_bytes: common.endpoint.body_bytes,
            max_headers: common.endpoint.headers,
            max_header_bytes: common.endpoint.header_bytes,
            max_concurrent_evaluations,
        },
        ledger: common.ledger,
        ingress: common.ingress,
        replay: common.replay,
        signed_age: common.signed_age,
        future_skew: common.future_skew,
        http: common.http,
        git: common.git,
        runner: common.runner,
    })
}

fn checked_common(raw: &ExecutionLimits) -> Result<CommonLimits, ConfigError> {
    let headers = u64::try_from(raw.header_count)
        .map_err(|_defect| ConfigError("header count is too large"))?;
    let header_bytes = u64::try_from(raw.header_bytes)
        .map_err(|_defect| ConfigError("header byte limit is too large"))?;
    if !(1..=MAX_BODY_BYTES).contains(&raw.body_bytes)
        || !(1..=MAX_HEADERS).contains(&headers)
        || !(1..=MAX_HEADER_BYTES).contains(&header_bytes)
    {
        return Err(ConfigError("endpoint limits are invalid"));
    }
    let (replay, ingress, future_skew) = checked_ingress(raw)?;
    let http = checked_http(raw)?;
    let (ledger, git, runner) = checked_execution(raw, replay, http)?;
    let endpoint = EndpointLimits {
        body_bytes: raw.body_bytes,
        headers,
        header_bytes,
    };
    Ok(CommonLimits {
        endpoint,
        ledger,
        ingress,
        replay,
        signed_age: SIGNED_AGE,
        future_skew,
        http,
        git,
        runner,
    })
}

fn checked_ingress(
    raw: &ExecutionLimits,
) -> Result<(ReplayWindow, IngressPolicy, Duration), ConfigError> {
    let replay = ReplayWindow::new(SIGNED_AGE, Duration::from_secs(raw.queue_age_seconds))
        .ok_or(ConfigError("replay limits are invalid"))?;
    let ingress_limits = IngressLimits::new(raw.body_bytes, raw.header_count, raw.header_bytes)
        .ok_or(ConfigError("ingress limits are invalid"))?;
    let future_skew = Duration::from_secs(raw.future_skew_seconds);
    if future_skew > MAX_FUTURE_SKEW {
        return Err(ConfigError("future skew is too large"));
    }
    let ingress = IngressPolicy::new(ingress_limits, replay, future_skew)
        .ok_or(ConfigError("ingress time limits are invalid"))?;
    Ok((replay, ingress, future_skew))
}
