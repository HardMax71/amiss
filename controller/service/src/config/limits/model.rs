use std::time::Duration;

use amiss_controller::{IngressPolicy, ReplayWindow};

use crate::{EvaluationConfig, InboxLimits, ReceiverConfig};

pub struct LoadedLimits {
    pub receiver: ReceiverConfig,
    pub inbox: InboxLimits,
    pub ledger: LedgerLimits,
    pub ingress: IngressPolicy,
    pub replay: ReplayWindow,
    pub signed_age: Duration,
    pub future_skew: Duration,
    pub http: HttpLimits,
    pub git: GitLimits,
    pub runner: RunnerLimits,
    pub worker: WorkerLimits,
}

pub struct LoadedExecutionLimits {
    pub evaluation: EvaluationConfig,
    pub ledger: LedgerLimits,
    pub ingress: IngressPolicy,
    pub replay: ReplayWindow,
    pub signed_age: Duration,
    pub future_skew: Duration,
    pub http: HttpLimits,
    pub git: GitLimits,
    pub runner: RunnerLimits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedgerLimits {
    pub lease: Duration,
    pub records: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HttpLimits {
    pub connect: Duration,
    pub read: Duration,
    pub write: Duration,
    pub request: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitLimits {
    pub request: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunnerLimits {
    pub bootstrap: Duration,
    pub statement_validity: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerLimits {
    pub retry_min: Duration,
    pub retry_max: Duration,
    pub idle_poll: Duration,
}

#[derive(Clone, Copy)]
pub(super) struct CommonLimits {
    pub(super) endpoint: EndpointLimits,
    pub(super) ledger: LedgerLimits,
    pub(super) ingress: IngressPolicy,
    pub(super) replay: ReplayWindow,
    pub(super) signed_age: Duration,
    pub(super) future_skew: Duration,
    pub(super) http: HttpLimits,
    pub(super) git: GitLimits,
    pub(super) runner: RunnerLimits,
}

#[derive(Clone, Copy)]
pub(super) struct EndpointLimits {
    pub(super) body_bytes: usize,
    pub(super) headers: u64,
    pub(super) header_bytes: u64,
}
