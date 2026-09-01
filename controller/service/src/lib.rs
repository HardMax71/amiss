#![forbid(unsafe_code)]

mod artifact;
mod config;
mod delivery;
mod endpoint;
mod error;
mod evaluation;
mod frame;
mod hash;
mod inbox;
mod lane;
mod launcher;
mod limits;
mod objects;
mod operations;
mod probe;
mod queued;
mod receiver;
mod record;
mod relation;
mod shutdown;
mod store;
mod supervision;
mod worker;

pub use artifact::{
    ArtifactFiles, ArtifactService, ArtifactServiceConfig, artifact_routes, load_artifact_service,
    open_artifact_service,
};
pub use config::{
    ArtifactLimits, CheckPlanFiles, ConfigError, ExecutionLimits, ExecutionPaths, GitLimits,
    HttpLimits, LedgerLimits, LoadedExecutionLimits, LoadedExecutionPaths, LoadedLimits,
    LoadedPaths, RunnerLimits, ServiceLimits, ServicePaths, WebhookKeyFile, WorkerLimits,
    framed_route_id, load_execution_limits, load_execution_paths, load_limits, load_paths,
    load_plan, load_relation_registry, load_webhook_keyring, read_regular, read_strict_json,
};
pub use delivery::{Delivery, DeliveryHeader, IncomingDelivery, IncomingHeader};
pub use endpoint::{EndpointConfig, EndpointConfigError};
pub use error::InboxError;
pub use evaluation::{EvaluationRequest, evaluation_router, evaluation_router_with_clock};
pub use inbox::{
    ClaimOutcome, ClaimedDelivery, CompleteOutcome, DeliveryLease, EnqueueOutcome, Inbox,
    InboxEntry, InboxState, RenewOutcome, RetryOutcome,
};
pub use lane::{check_lane, repository_admission};
pub use launcher::service_main;
pub use limits::InboxLimits;
pub use objects::{GitObjectSource, ResolveWant, ResolvedCommit};
pub use operations::{Operations, ServiceComponent, ServiceEvent};
pub use probe::EndpointDrain;
pub use queued::{
    QueuedLaneSetupError, QueuedLaneSetupInput, QueuedService, QueuedServiceError,
    QueuedServiceSettings, run_queued_service, setup_repository_lane,
};
pub use receiver::{
    AdmissionRejection, AdmissionRequest, AdmittedDelivery, DeliveryAdmission, router,
    router_with_clock, serve,
};
pub use relation::{
    CoordinatedRelation, CoordinatedTransition, admit_relation_coordination,
    freeze_relation_transition,
};
pub use shutdown::shutdown_signal;
pub use supervision::{Supervision, SupervisionError, supervise};
pub use worker::{
    AcquiringWorkerBuildError, AcquiringWorkerContext, AcquiringWorkerSettings, DeliveryWorker,
    DeliveryWorkerError, DeliveryWorkerInput, WorkOutcome, acquiring_worker,
};
