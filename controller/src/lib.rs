#![forbid(unsafe_code)]

mod acquiring_runner;
mod acquisition;
#[doc(hidden)]
pub mod atomic_write_recovery;
mod bootstrap_job;
mod bootstrap_result;
mod bootstrap_runner;
mod bounded_json;
mod clock;
mod external;
pub mod feedback;
mod file_ledger;
mod identity;
mod ingress;
mod orchestration;
mod plans;
mod provider;
mod spelling;
mod webhook;

pub use acquiring_runner::{AcquiringRunner, Acquisition, AcquisitionTarget};
pub use acquisition::{AcquireError, AcquiredRoots, verify_acquired};
pub use amiss_bootstrap::BOOTSTRAP_EXECUTABLE_BYTES;
pub use bootstrap_job::{
    AcquiredControl, BootstrapJob, BootstrapJobError, BootstrapJobInput, CheckBinding, CheckPlan,
    PolicyControls, bootstrap_job, check_binding, check_plan,
};
pub use bootstrap_result::{BootstrapTermination, classify_bootstrap_result};
pub use bootstrap_runner::{BootstrapRun, run_bootstrap};
pub use bounded_json::decode_bounded_json;
pub use clock::{ControllerClock, SystemClock};
pub use external::{
    ForgeEvidence, ForgePresence, ForgeProducer, ForgeRefFamily, ForgeTail, ForgeTarget,
    ForgeVisibility, forge_evidence, forge_repository_evidence,
};
pub use file_ledger::{
    FileLedger, FileLedgerCleanup, FileLedgerConfig, FileLedgerError, FileLedgerRoot,
};
pub use identity::{
    ChangeId, ChangeLocator, ControllerEvaluationId, DeliveryId, DeliveryIdentity, IntegrationId,
    OpaqueId, ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt,
    ProviderRunId, ProviderRunIdentity,
};
pub use ingress::{
    AcceptedDelivery, DeliveryHeader, DeliveryRoute, IngressCheck, IngressError, IngressLimits,
    IngressPolicy, ReplayIdentity, ReplayWindow, SignedTimePolicy, TrustAnchorId, TrustSetId,
    UntrustedDelivery, VerifiedDelivery,
};
pub use orchestration::{
    ChangeSnapshot, ChangeState, CheckConclusion, Controller, ControllerError, DeliveryClaim,
    DeliveryLease, DeliveryLedger, Evaluation, ExternalSink, ExternalTally, HandleOutcome,
    HeartbeatOutcome, LeaseCompletion, LeaseFence, LeaseRenewal, OidPair, Publication, RunFailure,
    RunHeartbeat, RunIdentity, RunRefs, RunRequest, Runner, RunnerOutcome, StageOutcome,
    StagedPublication,
};
pub use plans::{PlanError, PlanRegistry, PlanScope, ResolvedPlan, register_plan, resolve_plan};
pub use provider::{
    AdapterRegistry, AuthenticatedDelivery, ForgeFact, ForgeNegative, OperationDeadline,
    ProviderAdapter, ProviderError, RegistryError,
};
pub use spelling::{ref_span, spelled_segments};
pub use webhook::{
    GitHubWebhook, GitLabWebhook, GiteaWebhook, SignedRequestProof, WebhookError, WebhookKey,
    WebhookKeyring, WebhookKeyringError, WebhookProof,
};
