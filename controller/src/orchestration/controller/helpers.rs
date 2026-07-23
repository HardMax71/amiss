use std::time::Duration;

use amiss_wire::model::Oid;

use crate::{
    AcceptedDelivery, AuthenticatedDelivery, CheckBinding, ControllerClock, ProviderAdapter,
};

use super::{ControllerError, HandleOutcome};
use crate::orchestration::ledger::{
    DeliveryLease, DeliveryLedger, LeaseCompletion, LeaseRenewal, Publication, StageOutcome,
    StagedPublication,
};
use crate::orchestration::model::{ChangeSnapshot, HeartbeatOutcome, RunHeartbeat, RunIdentity};

pub(super) struct LedgerHeartbeat<'a, L: DeliveryLedger> {
    ledger: &'a mut L,
    delivery: &'a AcceptedDelivery,
    lease: &'a mut DeliveryLease,
    clock: &'a dyn ControllerClock,
    failure: Option<ControllerError<L::Error>>,
}

impl<'a, L: DeliveryLedger> LedgerHeartbeat<'a, L> {
    pub(super) fn new(
        ledger: &'a mut L,
        delivery: &'a AcceptedDelivery,
        lease: &'a mut DeliveryLease,
        clock: &'a dyn ControllerClock,
    ) -> Self {
        Self {
            ledger,
            delivery,
            lease,
            clock,
            failure: None,
        }
    }

    pub(super) fn finish(self) -> Result<(), ControllerError<L::Error>> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl<L: DeliveryLedger> RunHeartbeat for LedgerHeartbeat<'_, L> {
    fn renew(&mut self) -> HeartbeatOutcome {
        if self.failure.is_some() {
            return HeartbeatOutcome::Stop;
        }
        let renewed = renew_lease(self.ledger, self.delivery, self.lease).and_then(|lease| {
            let renew_within =
                renewal_window(&lease, self.clock).ok_or(ControllerError::LeaseLost)?;
            Ok((lease, renew_within))
        });
        match renewed {
            Ok((lease, renew_within)) => {
                *self.lease = lease;
                HeartbeatOutcome::Renewed { renew_within }
            }
            Err(error) => {
                self.failure = Some(error);
                HeartbeatOutcome::Stop
            }
        }
    }
}

fn renewal_window(lease: &DeliveryLease, clock: &dyn ControllerClock) -> Option<Duration> {
    let now = clock.now_unix_millis()?;
    let millis = u64::try_from(lease.expires_at_unix_millis.checked_sub(now)?).ok()?;
    let remaining = Duration::from_millis(millis);
    (!remaining.is_zero()).then_some(remaining)
}

pub(super) fn renew_lease<L: DeliveryLedger>(
    ledger: &mut L,
    delivery: &AcceptedDelivery,
    lease: &DeliveryLease,
) -> Result<DeliveryLease, ControllerError<L::Error>> {
    let renewal = ledger
        .renew(delivery, lease)
        .map_err(ControllerError::Ledger)?;
    let LeaseRenewal::Renewed(renewed) = renewal else {
        return Err(ControllerError::LeaseLost);
    };
    if renewed.evaluation_id != lease.evaluation_id
        || renewed.check != lease.check
        || renewed.fence != lease.fence
        || renewed.expires_at_unix_millis < lease.expires_at_unix_millis
    {
        return Err(ControllerError::LeaseLost);
    }
    Ok(renewed)
}

pub(super) fn stage_publication<L: DeliveryLedger>(
    ledger: &mut L,
    delivery: &AcceptedDelivery,
    lease: &DeliveryLease,
    publication: &Publication,
) -> Result<StagedPublication, ControllerError<L::Error>> {
    let outcome = ledger
        .stage(delivery, lease, publication)
        .map_err(ControllerError::Ledger)?;
    match outcome {
        StageOutcome::Staged(staged) if staged.publication.as_ref() == publication => {
            validate_staged_lease(lease, staged)
        }
        StageOutcome::Staged(_) | StageOutcome::Lost => Err(ControllerError::LeaseLost),
    }
}

fn validate_staged_lease<E>(
    lease: &DeliveryLease,
    staged: StagedPublication,
) -> Result<StagedPublication, ControllerError<E>> {
    if staged.evaluation_id != lease.evaluation_id
        || staged.fence != lease.fence
        || staged.publication.check != lease.check
    {
        return Err(ControllerError::LeaseLost);
    }
    Ok(staged)
}

pub(super) fn publish_staged<L: DeliveryLedger>(
    adapter: &dyn ProviderAdapter,
    ledger: &mut L,
    delivery: &AcceptedDelivery,
    staged: &StagedPublication,
) -> Result<HandleOutcome, ControllerError<L::Error>> {
    adapter
        .publish(delivery.delivery(), &staged.publication)
        .map_err(ControllerError::Publish)?;
    match ledger
        .complete(delivery, staged)
        .map_err(ControllerError::Completion)?
    {
        LeaseCompletion::Completed => Ok(HandleOutcome::Published(staged.publication.conclusion)),
        LeaseCompletion::Lost => Err(ControllerError::CompletionLost),
    }
}

pub(super) fn validate_staged<E>(
    delivery: &AuthenticatedDelivery,
    check: &CheckBinding,
    staged: &StagedPublication,
) -> Result<(), ControllerError<E>> {
    if staged.publication.evaluation_id != staged.evaluation_id {
        return Err(ControllerError::LeaseLost);
    }
    if staged.publication.provider_run != delivery.provider_run {
        return Err(ControllerError::WrongProviderRun);
    }
    if staged.publication.check != *check {
        return Err(ControllerError::DeliveryBindingConflict);
    }
    validate_run(delivery, &staged.publication.run)?;
    validate_gate_commit(&staged.publication.run, &staged.publication.gate_commit)
}

pub(super) fn validate_change<E>(
    delivery: &AuthenticatedDelivery,
    snapshot: &ChangeSnapshot,
) -> Result<(), ControllerError<E>> {
    validate_run(delivery, &snapshot.run)?;
    validate_gate_commit(&snapshot.run, &snapshot.gate_commit)
}

fn validate_gate_commit<E>(run: &RunIdentity, gate_commit: &Oid) -> Result<(), ControllerError<E>> {
    Oid::new(run.object_format, gate_commit.as_str().to_owned())
        .ok_or(ControllerError::WrongProviderRun)?;
    Ok(())
}

fn validate_run<E>(
    delivery: &AuthenticatedDelivery,
    run: &RunIdentity,
) -> Result<(), ControllerError<E>> {
    if run.change != delivery.change {
        return Err(ControllerError::WrongChangeIdentity);
    }
    if run.object_format != delivery.provider_run.object_format
        || run.commits.candidate != delivery.provider_run.candidate_commit
    {
        return Err(ControllerError::WrongProviderRun);
    }
    Ok(())
}
