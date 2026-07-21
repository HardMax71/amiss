#![forbid(unsafe_code)]

mod identity;
mod orchestration;
mod provider;

pub use identity::{
    ChangeId, ChangeLocator, ControllerEvaluationId, DeliveryId, DeliveryIdentity, IntegrationId,
    OpaqueId, ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt,
    ProviderRunId, ProviderRunIdentity,
};
pub use orchestration::{
    ChangeSnapshot, ChangeState, CheckConclusion, Controller, ControllerError, DeliveryClaim,
    DeliveryLease, DeliveryLedger, Evaluation, HandleOutcome, LeaseCompletion, LeaseFence,
    LeaseRenewal, OidPair, Publication, RunFailure, RunIdentity, RunRefs, RunRequest, Runner,
    RunnerOutcome, StageOutcome, StagedPublication,
};
pub use provider::{
    AdapterRegistry, AuthenticatedDelivery, DeliveryHeader, ProviderAdapter, ProviderError,
    RegistryError, UntrustedDelivery,
};
