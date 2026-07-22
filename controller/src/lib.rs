#![forbid(unsafe_code)]

mod clock;
mod file_ledger;
mod identity;
mod ingress;
mod orchestration;
mod provider;
mod webhook;

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
pub use provider::{
    AdapterRegistry, AuthenticatedDelivery, ProviderAdapter, ProviderError, RegistryError,
};
pub use webhook::{
    GitHubWebhook, GitLabWebhook, GiteaWebhook, WebhookError, WebhookKey, WebhookKeyring,
    WebhookKeyringError, WebhookProof,
};
