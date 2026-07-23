#![forbid(unsafe_code)]

mod config;
mod delivery;
mod error;
mod evaluation;
mod frame;
mod hash;
mod inbox;
mod lane;
mod limits;
mod queued;
mod receiver;
mod record;
mod store;
mod worker;

pub use config::{
    CheckPlanFiles, ConfigError, ExecutionLimits, ExecutionPaths, GitLimits, HttpLimits,
    LedgerLimits, LoadedExecutionLimits, LoadedExecutionPaths, LoadedLimits, LoadedPaths,
    RunnerLimits, ServiceLimits, ServicePaths, WebhookKeyFile, WorkerLimits, framed_route_id,
    load_execution_limits, load_execution_paths, load_limits, load_paths, load_plan,
    load_webhook_keyring, read_regular, read_strict_json,
};
pub use delivery::{Delivery, DeliveryHeader, IncomingDelivery, IncomingHeader};
pub use error::InboxError;
pub use evaluation::{
    EvaluationConfig, EvaluationConfigError, EvaluationRequest, evaluation_router,
};
pub use inbox::{
    ClaimOutcome, ClaimedDelivery, CompleteOutcome, DeliveryLease, EnqueueOutcome, Inbox,
    InboxEntry, InboxState, RenewOutcome, RetryOutcome,
};
pub use lane::{LaneAdmission, check_lane, lane_admission};
pub use limits::InboxLimits;
pub use queued::{QueuedServiceError, QueuedServiceInput, run_queued_service};
pub use receiver::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission, ReceiverConfig,
    ReceiverConfigError, router, serve,
};
pub use worker::{DeliveryWorker, DeliveryWorkerError, DeliveryWorkerInput, WorkOutcome};
