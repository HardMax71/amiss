#![forbid(unsafe_code)]

mod bootstrap_job;
mod clock;
mod file_ledger;
mod identity;
mod ingress;
mod orchestration;
mod plans;
mod provider;
mod webhook;

pub use bootstrap_job::{
    AcquiredControl, BootstrapJob, BootstrapJobError, BootstrapJobInput, CheckBinding, CheckPlan,
    PolicyControls, bootstrap_job, check_binding, check_plan,
};
pub use clock::{ControllerClock, SystemClock};
pub use file_ledger::{FileLedger, FileLedgerCleanup, FileLedgerConfig, FileLedgerError};
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
    DeliveryLease, DeliveryLedger, Evaluation, HandleOutcome, HeartbeatOutcome, LeaseCompletion,
    LeaseFence, LeaseRenewal, OidPair, Publication, RunFailure, RunHeartbeat, RunIdentity, RunRefs,
    RunRequest, Runner, RunnerOutcome, StageOutcome, StagedPublication,
};
pub use plans::{PlanError, PlanRegistry, PlanScope, ResolvedPlan, register_plan, resolve_plan};
pub use provider::{
    AdapterRegistry, AuthenticatedDelivery, ProviderAdapter, ProviderError, RegistryError,
};
pub use webhook::{
    GitHubWebhook, GitLabWebhook, GiteaWebhook, WebhookError, WebhookKey, WebhookKeyring,
    WebhookKeyringError, WebhookProof,
};
