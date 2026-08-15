mod helpers;

use std::sync::Arc;

use crate::{
    AdapterRegistry, ControllerClock, ControllerEvaluationId, IngressError, IngressPolicy,
    PlanError, PlanRegistry, ProviderError, ResolvedPlan, SystemClock, UntrustedDelivery,
    resolve_plan,
};

use self::helpers::{
    LedgerHeartbeat, observe_external, publish_staged, renew_lease, stage_publication,
    validate_change, validate_staged,
};
use super::ledger::{CheckConclusion, DeliveryClaim, DeliveryLedger};
use super::model::{ChangeState, RunRequest, Runner};
use super::publication::publication;

#[derive(Debug, thiserror::Error)]
pub enum ControllerError<E> {
    #[error("no adapter handles the provider")]
    UnknownProvider,
    #[error("check plan selection failed: {0}")]
    Plan(#[source] PlanError),
    #[error("provider ingress failed: {0}")]
    Ingress(#[source] IngressError),
    #[error("provider operation failed: {0}")]
    Provider(#[source] ProviderError),
    #[error("provider refresh changed the authenticated change identity")]
    WrongChangeIdentity,
    #[error("provider refresh changed the authenticated provider run")]
    WrongProviderRun,
    #[error("delivery key was rebound to another run or check plan")]
    DeliveryBindingConflict,
    #[error("delivery lease is no longer authoritative")]
    LeaseLost,
    #[error("published result lost its staged completion record")]
    CompletionLost,
    #[error("delivery ledger operation failed: {0}")]
    Ledger(#[source] E),
    #[error("published result could not be completed: {0}")]
    Completion(#[source] E),
    #[error("provider publication failed: {0}")]
    Publish(#[source] ProviderError),
}

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

/// The advisory verdict counts of one published delivery's external
/// assessment; the verdict the provider shows is already sealed when these
/// are tallied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExternalTally {
    pub refuted: u64,
    pub unproven: u64,
    pub reachable: u64,
}

/// Receives the advisory external outcome after a delivery published. No
/// sink means no verification is attempted at all.
pub trait ExternalSink: Send + Sync {
    fn assessed(&self, tally: &ExternalTally);

    /// Verification was owed but could not finish; nothing was tallied.
    fn incomplete(&self);
}

pub struct Controller<L, R> {
    pub registry: AdapterRegistry,
    pub plans: PlanRegistry,
    pub ledger: L,
    pub runner: R,
    ingress: IngressPolicy,
    clock: Arc<dyn ControllerClock>,
    external: Option<Arc<dyn ExternalSink>>,
}

impl<L, R> Controller<L, R>
where
    L: DeliveryLedger,
    R: Runner,
{
    pub fn new(
        registry: AdapterRegistry,
        plans: PlanRegistry,
        ledger: L,
        runner: R,
        ingress: IngressPolicy,
    ) -> Self {
        let clock: Arc<dyn ControllerClock> = Arc::new(SystemClock);
        Self::new_with_clock(registry, plans, ledger, runner, ingress, clock)
    }

    pub fn new_with_clock(
        registry: AdapterRegistry,
        plans: PlanRegistry,
        ledger: L,
        runner: R,
        ingress: IngressPolicy,
        clock: Arc<dyn ControllerClock>,
    ) -> Self {
        Self {
            registry,
            plans,
            ledger,
            runner,
            ingress,
            clock,
            external: None,
        }
    }

    /// Turns on advisory external verification after publications, with the
    /// sink receiving each delivery's tally.
    #[must_use]
    pub fn with_external_sink(mut self, sink: Arc<dyn ExternalSink>) -> Self {
        self.external = Some(sink);
        self
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
        let ResolvedPlan { plan, check } =
            resolve_plan(&self.plans, delivery).map_err(ControllerError::Plan)?;
        let mut lease = match self
            .ledger
            .claim(&accepted, &check)
            .map_err(ControllerError::Ledger)?
        {
            DeliveryClaim::Execute(lease) if lease.check == check => lease,
            DeliveryClaim::Execute(_) => return Err(ControllerError::LeaseLost),
            DeliveryClaim::Publish(staged) => {
                validate_staged(delivery, &check, &staged)?;
                let outcome = publish_staged(adapter, &mut self.ledger, &accepted, &staged)?;
                observe_external(
                    adapter,
                    self.external.as_deref(),
                    self.clock.as_ref(),
                    &staged.publication,
                );
                return Ok(outcome);
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
            check,
            plan,
            run: initial.run.clone(),
        };
        let runner_outcome = match initial.state {
            ChangeState::Active => {
                let mut heartbeat = LedgerHeartbeat::new(
                    &mut self.ledger,
                    &accepted,
                    &mut lease,
                    self.clock.as_ref(),
                );
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
        let outcome = publish_staged(adapter, &mut self.ledger, &accepted, &staged)?;
        observe_external(
            adapter,
            self.external.as_deref(),
            self.clock.as_ref(),
            &staged.publication,
        );
        Ok(outcome)
    }
}
