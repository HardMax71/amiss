use std::fmt;
use std::num::NonZeroU64;

use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};
use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::{
    AdapterRegistry, AuthenticatedDelivery, ChangeLocator, ControllerEvaluationId,
    DeliveryIdentity, ProviderError, ProviderRunIdentity, UntrustedDelivery,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeState {
    Active,
    Superseded,
    Closed,
    AuthorizationRevoked,
}

/// The refs one run resolves against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRefs {
    pub forge: ForgeDialect,
    pub candidate: BranchRef,
    pub target: BranchRef,
    pub default_branch: BranchRef,
}

/// One base and candidate pair of object ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OidPair {
    pub base: Oid,
    pub candidate: Oid,
}

impl OidPair {
    fn well_formed(&self, object_format: ObjectFormat) -> bool {
        [&self.base, &self.candidate]
            .into_iter()
            .all(|oid| Oid::new(object_format, oid.as_str().to_owned()).is_some())
    }
}

/// The exact identity one evaluation runs as. Everything here is data; the
/// binding laws live in `validate_change` and the runner recheck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunIdentity {
    pub change: ChangeLocator,
    pub refs: RunRefs,
    pub object_format: ObjectFormat,
    pub commits: OidPair,
    pub trees: OidPair,
}

impl RunIdentity {
    /// None unless every oid is well formed for the object format.
    pub fn new(
        change: ChangeLocator,
        refs: RunRefs,
        object_format: ObjectFormat,
        commits: OidPair,
        trees: OidPair,
    ) -> Option<Self> {
        if !commits.well_formed(object_format) || !trees.well_formed(object_format) {
            return None;
        }
        Some(Self {
            change,
            refs,
            object_format,
            commits,
            trees,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSnapshot {
    pub state: ChangeState,
    pub run: RunIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRequest {
    pub delivery: DeliveryIdentity,
    pub provider_run: ProviderRunIdentity,
    pub evaluation_id: ControllerEvaluationId,
    pub run: RunIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evaluation {
    Pass,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunFailure {
    MissingOutput,
    Timeout,
    TamperedRuntime,
    Unavailable,
    OversizedOutput,
    WrongIdentity,
    WrongTree,
    AuthorizationRevoked,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerOutcome {
    Complete {
        identity: Box<RunIdentity>,
        evaluation: Evaluation,
        report: Vec<u8>,
    },
    MissingOutput,
    TimedOut,
    TamperedRuntime,
    Unavailable,
}

pub trait Runner {
    /// Runs the exact acquired identity. `Complete` is reserved for a report
    /// whose engine, exit class, and request bindings the trusted runner has
    /// already accepted; the controller independently rechecks the identity.
    fn run(&mut self, request: &RunRequest) -> RunnerOutcome;
}

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

    /// # Errors
    ///
    /// Atomically creates, resumes, or fences a durable lease. Every reclaim
    /// keeps the first evaluation ID and advances the fence. A live lease held
    /// by another ledger owner returns `Busy`; a frozen result returns
    /// `Publish`; `Duplicate` is reserved for a terminal, durably completed
    /// delivery. Reusing one delivery key for a different authenticated change
    /// or provider run returns `BindingConflict`.
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

#[derive(Debug)]
pub enum ControllerError<E> {
    UnknownProvider,
    Provider(ProviderError),
    WrongAuthenticatedProvider,
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
            Self::Provider(error) => write!(formatter, "provider operation failed: {error}"),
            Self::WrongAuthenticatedProvider => {
                formatter.write_str("authenticated provider does not match the routed provider")
            }
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
}

impl<L, R> Controller<L, R>
where
    L: DeliveryLedger,
    R: Runner,
{
    pub const fn new(registry: AdapterRegistry, ledger: L, runner: R) -> Self {
        Self {
            registry,
            ledger,
            runner,
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
        let adapter = self
            .registry
            .get(&input.expected_provider.namespace)
            .ok_or(ControllerError::UnknownProvider)?;
        let delivery = adapter
            .authenticate(input)
            .map_err(ControllerError::Provider)?;
        if delivery.identity.provider != *input.expected_provider
            || delivery.change.provider != *input.expected_provider
        {
            return Err(ControllerError::WrongAuthenticatedProvider);
        }
        let mut lease = match self
            .ledger
            .claim(&delivery)
            .map_err(ControllerError::Ledger)?
        {
            DeliveryClaim::Execute(lease) => lease,
            DeliveryClaim::Publish(staged) => {
                validate_staged(&delivery, &staged)?;
                return publish_staged(adapter, &mut self.ledger, &delivery, &staged);
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
            .refresh(&delivery)
            .map_err(ControllerError::Provider)?;
        validate_change(&delivery, &initial)?;
        lease = renew_lease(&mut self.ledger, &delivery, &lease)?;
        let request = RunRequest {
            delivery: delivery.identity.clone(),
            provider_run: delivery.provider_run.clone(),
            evaluation_id: lease.evaluation_id.clone(),
            run: initial.run.clone(),
        };
        let runner_outcome = match initial.state {
            ChangeState::Active => Some(self.runner.run(&request)),
            ChangeState::Superseded | ChangeState::Closed | ChangeState::AuthorizationRevoked => {
                None
            }
        };
        lease = renew_lease(&mut self.ledger, &delivery, &lease)?;
        let fresh = adapter
            .refresh(&delivery)
            .map_err(ControllerError::Provider)?;
        validate_change(&delivery, &fresh)?;
        lease = renew_lease(&mut self.ledger, &delivery, &lease)?;
        let publication = publication(&request, &initial, &fresh, runner_outcome);
        let staged = stage_publication(&mut self.ledger, &delivery, &lease, &publication)?;
        publish_staged(adapter, &mut self.ledger, &delivery, &staged)
    }
}

fn renew_lease<L: DeliveryLedger>(
    ledger: &mut L,
    delivery: &AuthenticatedDelivery,
    lease: &DeliveryLease,
) -> Result<DeliveryLease, ControllerError<L::Error>> {
    let renewal = ledger
        .renew(delivery, lease)
        .map_err(ControllerError::Ledger)?;
    let LeaseRenewal::Renewed(renewed) = renewal else {
        return Err(ControllerError::LeaseLost);
    };
    if renewed.evaluation_id != lease.evaluation_id
        || renewed.fence != lease.fence
        || renewed.expires_at_unix_millis < lease.expires_at_unix_millis
    {
        return Err(ControllerError::LeaseLost);
    }
    Ok(renewed)
}

fn stage_publication<L: DeliveryLedger>(
    ledger: &mut L,
    delivery: &AuthenticatedDelivery,
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
    if staged.evaluation_id != lease.evaluation_id || staged.fence != lease.fence {
        return Err(ControllerError::LeaseLost);
    }
    Ok(staged)
}

fn publish_staged<L: DeliveryLedger>(
    adapter: &dyn crate::ProviderAdapter,
    ledger: &mut L,
    delivery: &AuthenticatedDelivery,
    staged: &StagedPublication,
) -> Result<HandleOutcome, ControllerError<L::Error>> {
    adapter
        .publish(delivery, &staged.publication)
        .map_err(ControllerError::Publish)?;
    match ledger
        .complete(delivery, staged)
        .map_err(ControllerError::Completion)?
    {
        LeaseCompletion::Completed => Ok(HandleOutcome::Published(staged.publication.conclusion)),
        LeaseCompletion::Lost => Err(ControllerError::CompletionLost),
    }
}

fn validate_staged<E>(
    delivery: &AuthenticatedDelivery,
    staged: &StagedPublication,
) -> Result<(), ControllerError<E>> {
    if staged.publication.evaluation_id != staged.evaluation_id {
        return Err(ControllerError::LeaseLost);
    }
    if staged.publication.provider_run != delivery.provider_run {
        return Err(ControllerError::WrongProviderRun);
    }
    validate_run(delivery, &staged.publication.run)
}

fn validate_change<E>(
    delivery: &AuthenticatedDelivery,
    snapshot: &ChangeSnapshot,
) -> Result<(), ControllerError<E>> {
    validate_run(delivery, &snapshot.run)
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

fn publication(
    request: &RunRequest,
    initial: &ChangeSnapshot,
    fresh: &ChangeSnapshot,
    outcome: Option<RunnerOutcome>,
) -> Publication {
    let (conclusion, report) = if fresh.state == ChangeState::AuthorizationRevoked
        || initial.state == ChangeState::AuthorizationRevoked
    {
        (
            CheckConclusion::Unavailable(RunFailure::AuthorizationRevoked),
            None,
        )
    } else if fresh.state == ChangeState::Closed || initial.state == ChangeState::Closed {
        (CheckConclusion::Unavailable(RunFailure::Closed), None)
    } else if fresh.state == ChangeState::Superseded
        || initial.state == ChangeState::Superseded
        || initial.run != fresh.run
    {
        (CheckConclusion::Superseded, None)
    } else {
        runner_conclusion(&initial.run, outcome)
    };
    Publication {
        provider_run: request.provider_run.clone(),
        evaluation_id: request.evaluation_id.clone(),
        run: initial.run.clone(),
        conclusion,
        report,
    }
}

fn runner_conclusion(
    expected: &RunIdentity,
    outcome: Option<RunnerOutcome>,
) -> (CheckConclusion, Option<Vec<u8>>) {
    match outcome {
        Some(RunnerOutcome::Complete { identity, .. })
            if identity.change != expected.change
                || identity.refs != expected.refs
                || identity.object_format != expected.object_format
                || identity.commits != expected.commits =>
        {
            (
                CheckConclusion::Unavailable(RunFailure::WrongIdentity),
                None,
            )
        }
        Some(RunnerOutcome::Complete { identity, .. }) if identity.trees != expected.trees => {
            (CheckConclusion::Unavailable(RunFailure::WrongTree), None)
        }
        Some(RunnerOutcome::Complete { report, .. }) if report.is_empty() => (
            CheckConclusion::Unavailable(RunFailure::MissingOutput),
            None,
        ),
        Some(RunnerOutcome::Complete { report, .. })
            if u64::try_from(report.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES =>
        {
            (
                CheckConclusion::Unavailable(RunFailure::OversizedOutput),
                None,
            )
        }
        Some(RunnerOutcome::Complete {
            evaluation, report, ..
        }) => (
            match evaluation {
                Evaluation::Pass => CheckConclusion::Pass,
                Evaluation::Block => CheckConclusion::Block,
            },
            Some(report),
        ),
        Some(RunnerOutcome::MissingOutput) | None => (
            CheckConclusion::Unavailable(RunFailure::MissingOutput),
            None,
        ),
        Some(RunnerOutcome::TimedOut) => (CheckConclusion::Unavailable(RunFailure::Timeout), None),
        Some(RunnerOutcome::TamperedRuntime) => (
            CheckConclusion::Unavailable(RunFailure::TamperedRuntime),
            None,
        ),
        Some(RunnerOutcome::Unavailable) => {
            (CheckConclusion::Unavailable(RunFailure::Unavailable), None)
        }
    }
}
