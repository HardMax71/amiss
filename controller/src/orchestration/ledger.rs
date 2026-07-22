use std::num::NonZeroU64;

use crate::{AuthenticatedDelivery, ControllerEvaluationId, ProviderRunIdentity};

use super::model::{RunFailure, RunIdentity};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LeaseFence(NonZeroU64);

impl LeaseFence {
    pub const fn new(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(fence) => Some(Self(fence)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryLease {
    pub evaluation_id: ControllerEvaluationId,
    pub fence: LeaseFence,
    /// Advisory deadline; only the ledger transaction decides ownership.
    pub expires_at_unix_millis: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryClaim {
    Execute(DeliveryLease),
    Publish(StagedPublication),
    Busy {
        evaluation_id: ControllerEvaluationId,
        retry_at_unix_millis: i64,
    },
    Duplicate {
        evaluation_id: ControllerEvaluationId,
    },
    BindingConflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaseRenewal {
    Renewed(DeliveryLease),
    Lost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageOutcome {
    Staged(StagedPublication),
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseCompletion {
    Completed,
    Lost,
}

pub trait DeliveryLedger {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Atomically creates, resumes, or fences a durable lease. Every reclaim
    /// keeps the first evaluation ID and advances the fence. A live lease held
    /// by another ledger owner returns `Busy`; a frozen result returns
    /// `Publish`; `Duplicate` is reserved for a terminal, durably completed
    /// delivery. Reusing one delivery key for a different authenticated change
    /// or provider run returns `BindingConflict`.
    ///
    /// # Errors
    ///
    /// Returns an error when that guarantee cannot be established.
    fn claim(&mut self, delivery: &AuthenticatedDelivery) -> Result<DeliveryClaim, Self::Error>;

    /// Extends one live lease without changing its evaluation ID or fence or
    /// moving its advisory deadline backward.
    ///
    /// # Errors
    ///
    /// Returns an error when durable ownership cannot be checked. Missing,
    /// expired, staged, completed, or superseded leases return `Lost`.
    fn renew(
        &mut self,
        delivery: &AuthenticatedDelivery,
        lease: &DeliveryLease,
    ) -> Result<LeaseRenewal, Self::Error>;

    /// Atomically checks the live fence and freezes the exact publication before
    /// external I/O. If staging wins a race with reclaim, every claim until
    /// completion returns that immutable publication and renewal returns `Lost`.
    /// If reclaim wins, this stale stage returns `Lost`. Repeating the same stage
    /// after an ambiguous acknowledgement returns the exact staged value.
    ///
    /// # Errors
    ///
    /// Returns an error when durable staging cannot be checked. Missing,
    /// expired, completed, or superseded leases return `Lost`.
    fn stage(
        &mut self,
        delivery: &AuthenticatedDelivery,
        lease: &DeliveryLease,
        publication: &Publication,
    ) -> Result<StageOutcome, Self::Error>;

    /// Atomically moves the exact staged evaluation to its terminal state.
    /// Concurrent claims observe either `Publish` before the transition or
    /// `Duplicate` after it, never `Execute` or `Busy` in between.
    ///
    /// # Errors
    ///
    /// Returns an error when the durable decision cannot be made. Missing,
    /// unstaged, or conflicting publications return `Lost`. Repeating completion
    /// for the same staged value is `Completed` so a caller may safely resume
    /// after an ambiguous commit acknowledgement.
    fn complete(
        &mut self,
        delivery: &AuthenticatedDelivery,
        staged: &StagedPublication,
    ) -> Result<LeaseCompletion, Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckConclusion {
    Pass,
    Block,
    Superseded,
    Unavailable(RunFailure),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    pub provider_run: ProviderRunIdentity,
    pub evaluation_id: ControllerEvaluationId,
    pub run: RunIdentity,
    pub conclusion: CheckConclusion,
    pub report: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedPublication {
    pub evaluation_id: ControllerEvaluationId,
    pub fence: LeaseFence,
    pub publication: Box<Publication>,
}
