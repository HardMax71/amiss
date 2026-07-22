use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use amiss_controller::{
    AcceptedDelivery, AuthenticatedDelivery, CheckBinding, ControllerEvaluationId, DeliveryClaim,
    DeliveryIdentity, DeliveryLease, DeliveryLedger, LeaseCompletion, LeaseFence, LeaseRenewal,
    Publication, StageOutcome, StagedPublication,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LedgerError;

impl fmt::Display for LedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("test ledger error")
    }
}

impl std::error::Error for LedgerError {}

#[derive(Clone)]
struct LedgerRow {
    binding: AuthenticatedDelivery,
    lease: DeliveryLease,
    staged: Option<StagedPublication>,
    complete: bool,
}

#[derive(Default)]
pub(crate) struct MemoryLedger {
    rows: BTreeMap<DeliveryIdentity, LedgerRow>,
    pub(crate) renewal_count: usize,
}

impl MemoryLedger {
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl DeliveryLedger for MemoryLedger {
    type Error = LedgerError;

    fn claim(
        &mut self,
        accepted: &AcceptedDelivery,
        check: &CheckBinding,
    ) -> Result<DeliveryClaim, Self::Error> {
        let delivery = accepted.delivery();
        if let Some(row) = self.rows.get(&delivery.identity) {
            if row.binding != *delivery || row.lease.check != *check {
                return Ok(DeliveryClaim::BindingConflict);
            }
            return if row.complete {
                Ok(DeliveryClaim::Duplicate {
                    evaluation_id: row.lease.evaluation_id.clone(),
                })
            } else if let Some(staged) = &row.staged {
                Ok(DeliveryClaim::Publish(staged.clone()))
            } else {
                Ok(DeliveryClaim::Execute(row.lease.clone()))
            };
        }
        let lease = lease_with(check.clone());
        self.rows.insert(
            delivery.identity.clone(),
            LedgerRow {
                binding: delivery.clone(),
                lease: lease.clone(),
                staged: None,
                complete: false,
            },
        );
        Ok(DeliveryClaim::Execute(lease))
    }

    fn renew(
        &mut self,
        accepted: &AcceptedDelivery,
        lease: &DeliveryLease,
    ) -> Result<LeaseRenewal, Self::Error> {
        let delivery = accepted.delivery();
        self.renewal_count = self.renewal_count.saturating_add(1);
        let Some(row) = self.rows.get_mut(&delivery.identity) else {
            return Ok(LeaseRenewal::Lost);
        };
        if row.binding == *delivery && row.lease == *lease && row.staged.is_none() && !row.complete
        {
            row.lease.expires_at_unix_millis = row.lease.expires_at_unix_millis.saturating_add(1);
            Ok(LeaseRenewal::Renewed(row.lease.clone()))
        } else {
            Ok(LeaseRenewal::Lost)
        }
    }

    fn complete(
        &mut self,
        accepted: &AcceptedDelivery,
        staged: &StagedPublication,
    ) -> Result<LeaseCompletion, Self::Error> {
        let delivery = accepted.delivery();
        let Some(row) = self.rows.get_mut(&delivery.identity) else {
            return Ok(LeaseCompletion::Lost);
        };
        if row.binding != *delivery || row.staged.as_ref() != Some(staged) {
            return Ok(LeaseCompletion::Lost);
        }
        row.complete = true;
        Ok(LeaseCompletion::Completed)
    }

    fn stage(
        &mut self,
        accepted: &AcceptedDelivery,
        lease: &DeliveryLease,
        publication: &Publication,
    ) -> Result<StageOutcome, Self::Error> {
        let delivery = accepted.delivery();
        let Some(row) = self.rows.get_mut(&delivery.identity) else {
            return Ok(StageOutcome::Lost);
        };
        if row.binding != *delivery || row.lease != *lease || row.complete {
            return Ok(StageOutcome::Lost);
        }
        let staged = StagedPublication {
            evaluation_id: lease.evaluation_id.clone(),
            fence: lease.fence,
            publication: Box::new(publication.clone()),
        };
        match &row.staged {
            Some(existing) if *existing == staged => Ok(StageOutcome::Staged(existing.clone())),
            Some(_) => Ok(StageOutcome::Lost),
            None => {
                row.staged = Some(staged.clone());
                Ok(StageOutcome::Staged(staged))
            }
        }
    }
}

pub(crate) struct ScriptedLedger {
    pub(crate) claim: Option<DeliveryClaim>,
    pub(crate) renewals: VecDeque<Result<LeaseRenewal, LedgerError>>,
    pub(crate) stage: Option<StageOutcome>,
    pub(crate) completion: LeaseCompletion,
}

impl DeliveryLedger for ScriptedLedger {
    type Error = LedgerError;

    fn claim(
        &mut self,
        _delivery: &AcceptedDelivery,
        _check: &CheckBinding,
    ) -> Result<DeliveryClaim, Self::Error> {
        self.claim.take().ok_or(LedgerError)
    }

    fn renew(
        &mut self,
        _delivery: &AcceptedDelivery,
        _lease: &DeliveryLease,
    ) -> Result<LeaseRenewal, Self::Error> {
        match self.renewals.pop_front() {
            Some(result) => result,
            None => Err(LedgerError),
        }
    }

    fn complete(
        &mut self,
        _delivery: &AcceptedDelivery,
        _staged: &StagedPublication,
    ) -> Result<LeaseCompletion, Self::Error> {
        Ok(self.completion)
    }

    fn stage(
        &mut self,
        _delivery: &AcceptedDelivery,
        _lease: &DeliveryLease,
        _publication: &Publication,
    ) -> Result<StageOutcome, Self::Error> {
        self.stage.take().ok_or(LedgerError)
    }
}

pub(crate) fn lease() -> DeliveryLease {
    lease_with(super::fixtures::binding())
}

fn lease_with(check: CheckBinding) -> DeliveryLease {
    DeliveryLease {
        evaluation_id: ControllerEvaluationId::new("evaluation-01".to_owned()).unwrap(),
        check,
        fence: LeaseFence::new(1).unwrap(),
        expires_at_unix_millis: 1_800_000_100_000,
    }
}

pub(crate) fn renewal_script(
    outcomes: impl IntoIterator<Item = LeaseRenewal>,
) -> VecDeque<Result<LeaseRenewal, LedgerError>> {
    outcomes.into_iter().map(Ok).collect()
}
