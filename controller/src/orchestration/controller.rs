mod helpers;

use std::fmt;
use std::sync::Arc;

use crate::{
    AdapterRegistry, ControllerClock, ControllerEvaluationId, IngressError, IngressPolicy,
    ProviderError, SystemClock, UntrustedDelivery,
};

use self::helpers::{
    LedgerHeartbeat, publish_staged, renew_lease, stage_publication, validate_change,
    validate_staged,
};
use super::ledger::{CheckConclusion, DeliveryClaim, DeliveryLedger};
use super::model::{ChangeState, RunRequest, Runner};
use super::publication::publication;

#[derive(Debug)]
pub enum ControllerError<E> {
    UnknownProvider,
    Ingress(IngressError),
    Provider(ProviderError),
    WrongChangeIdentity,
    WrongProviderRun,
    DeliveryBindingConflict,
    LeaseLost,
    CompletionLost,
    Ledger(E),
    Completion(E),
    Publish(ProviderError),
}

impl<E: fmt::Display> fmt::Display for ControllerError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProvider => formatter.write_str("no adapter handles the provider"),
            Self::Ingress(error) => write!(formatter, "provider ingress failed: {error}"),
            Self::Provider(error) => write!(formatter, "provider operation failed: {error}"),
            Self::WrongChangeIdentity => {
                formatter.write_str("provider refresh changed the authenticated change identity")
            }
            Self::WrongProviderRun => {
                formatter.write_str("provider refresh changed the authenticated provider run")
            }
            Self::DeliveryBindingConflict => {
                formatter.write_str("delivery key was rebound to another authenticated run")
            }
            Self::LeaseLost => formatter.write_str("delivery lease is no longer authoritative"),
            Self::CompletionLost => {
                formatter.write_str("published result lost its staged completion record")
            }
            Self::Ledger(error) => write!(formatter, "delivery ledger operation failed: {error}"),
            Self::Completion(error) => {
                write!(
                    formatter,
                    "published result could not be completed: {error}"
                )
            }
            Self::Publish(error) => write!(formatter, "provider publication failed: {error}"),
        }
    }
}

impl<E> std::error::Error for ControllerError<E> where E: std::error::Error + Send + Sync + 'static {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandleOutcome {
    InProgress {
        evaluation_id: ControllerEvaluationId,
        retry_at_unix_millis: i64,
    },
    Duplicate {
        evaluation_id: ControllerEvaluationId,
    },
    Published(CheckConclusion),
}

pub struct Controller<L, R> {
    pub registry: AdapterRegistry,
    pub ledger: L,
    pub runner: R,
    ingress: IngressPolicy,
    clock: Arc<dyn ControllerClock>,
}

impl<L, R> Controller<L, R>
where
    L: DeliveryLedger,
    R: Runner,
{
    pub fn new(registry: AdapterRegistry, ledger: L, runner: R, ingress: IngressPolicy) -> Self {
        Self::new_with_clock(registry, ledger, runner, ingress, Arc::new(SystemClock))
    }

    pub fn new_with_clock(
        registry: AdapterRegistry,
        ledger: L,
        runner: R,
        ingress: IngressPolicy,
        clock: Arc<dyn ControllerClock>,
    ) -> Self {
        Self {
            registry,
            ledger,
            runner,
            ingress,
            clock,
        }
    }

    /// Executes the provider-neutral trust flow from raw delivery through a
    /// fresh, exact provider result.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication, authoritative refresh, replay
    /// claiming, or publication cannot complete without guessing.
    pub fn handle(
        &mut self,
        input: UntrustedDelivery<'_>,
    ) -> Result<HandleOutcome, ControllerError<L::Error>> {
        let checked = self
            .ingress
            .pre_auth(input, self.clock.as_ref())
            .map_err(ControllerError::Ingress)?;
        let adapter = self
            .registry
            .get(&checked.delivery().route.provider.namespace)
            .ok_or(ControllerError::UnknownProvider)?;
        let verified = adapter
            .authenticate(checked)
            .map_err(ControllerError::Provider)?;
        let accepted = self
            .ingress
            .post_auth(checked, verified)
            .map_err(ControllerError::Ingress)?;
        let delivery = accepted.delivery();
        let mut lease = match self
            .ledger
            .claim(&accepted)
            .map_err(ControllerError::Ledger)?
        {
            DeliveryClaim::Execute(lease) => lease,
            DeliveryClaim::Publish(staged) => {
                validate_staged(delivery, &staged)?;
                return publish_staged(adapter, &mut self.ledger, &accepted, &staged);
            }
            DeliveryClaim::Busy {
                evaluation_id,
                retry_at_unix_millis,
            } => {
                return Ok(HandleOutcome::InProgress {
                    evaluation_id,
                    retry_at_unix_millis,
                });
            }
            DeliveryClaim::Duplicate { evaluation_id } => {
                return Ok(HandleOutcome::Duplicate { evaluation_id });
            }
            DeliveryClaim::BindingConflict => {
                return Err(ControllerError::DeliveryBindingConflict);
            }
        };
        let initial = adapter
            .refresh(delivery)
            .map_err(ControllerError::Provider)?;
        validate_change(delivery, &initial)?;
        lease = renew_lease(&mut self.ledger, &accepted, &lease)?;
        let request = RunRequest {
            delivery: delivery.identity.clone(),
            provider_run: delivery.provider_run.clone(),
            evaluation_id: lease.evaluation_id.clone(),
            run: initial.run.clone(),
        };
        let runner_outcome = match initial.state {
            ChangeState::Active => {
                let mut heartbeat = LedgerHeartbeat::new(&mut self.ledger, &accepted, &mut lease);
                let outcome = self.runner.run(&request, &mut heartbeat);
                heartbeat.finish()?;
                Some(outcome)
            }
            ChangeState::Superseded | ChangeState::Closed | ChangeState::AuthorizationRevoked => {
                None
            }
        };
        lease = renew_lease(&mut self.ledger, &accepted, &lease)?;
        let fresh = adapter
            .refresh(delivery)
            .map_err(ControllerError::Provider)?;
        validate_change(delivery, &fresh)?;
        lease = renew_lease(&mut self.ledger, &accepted, &lease)?;
        let publication = publication(&request, &initial, &fresh, runner_outcome);
        let staged = stage_publication(&mut self.ledger, &accepted, &lease, &publication)?;
        publish_staged(adapter, &mut self.ledger, &accepted, &staged)
    }
}
