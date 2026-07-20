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
    DeliveryLedger, Evaluation, HandleOutcome, OidPair, Publication, RunFailure, RunIdentity,
    RunRefs, RunRequest, Runner, RunnerOutcome,
};
pub use provider::{
    AdapterRegistry, AuthenticatedDelivery, DeliveryHeader, ProviderAdapter, ProviderError,
    RegistryError, UntrustedDelivery,
};
