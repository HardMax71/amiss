#![forbid(unsafe_code)]

mod identity;
mod orchestration;
mod provider;

pub use identity::{
    ChangeId, ChangeLocator, ControllerEvaluationId, DeliveryId, DeliveryIdentity, IntegrationId,
    ProviderIdentity, ProviderInstance, ProviderNamespace, ProviderRunAttempt, ProviderRunId,
    ProviderRunIdentity,
};
pub use orchestration::{
    ChangeSnapshot, ChangeState, CheckConclusion, Controller, ControllerError, DeliveryClaim,
    DeliveryLedger, Evaluation, HandleOutcome, Publication, RunFailure, RunIdentity, RunRequest,
    Runner, RunnerOutcome,
};
pub use provider::{
    AdapterRegistry, AuthenticatedDelivery, DeliveryHeader, ProviderAdapter, ProviderError,
    ProviderErrorKind, RegistryError, UntrustedDelivery,
};
