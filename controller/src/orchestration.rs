use std::fmt;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryClaim {
    Execute(ControllerEvaluationId),
    Duplicate,
}

pub trait DeliveryLedger {
    type Error: std::error::Error + Send + Sync + 'static;

    /// # Errors
    ///
    /// Atomically creates or resumes a durable lease. An incomplete delivery
    /// must return `Execute` with the same evaluation ID after a retry;
    /// `Duplicate` is reserved for a terminal, durably completed delivery.
    ///
    /// Returns an error when that guarantee cannot be established.
    fn claim(&mut self, delivery: &DeliveryIdentity) -> Result<DeliveryClaim, Self::Error>;

    /// Marks a published evaluation terminal under the claimed delivery key.
    ///
    /// # Errors
    ///
    /// Returns an error unless completion is durably recorded. Publication is
    /// idempotent by evaluation ID so a caller may safely resume after this
    /// operation fails.
    fn complete(
        &mut self,
        delivery: &DeliveryIdentity,
        evaluation_id: &ControllerEvaluationId,
    ) -> Result<(), Self::Error>;
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

#[derive(Debug)]
pub enum ControllerError<E> {
    UnknownProvider,
    Provider(ProviderError),
    WrongAuthenticatedProvider,
    WrongChangeIdentity,
    WrongProviderRun,
    Ledger(E),
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
            Self::Ledger(error) => write!(formatter, "delivery ledger operation failed: {error}"),
            Self::Publish(error) => write!(formatter, "provider publication failed: {error}"),
        }
    }
}

impl<E> std::error::Error for ControllerError<E> where E: std::error::Error + Send + Sync + 'static {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleOutcome {
    Duplicate,
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
        let evaluation_id = match self
            .ledger
            .claim(&delivery.identity)
            .map_err(ControllerError::Ledger)?
        {
            DeliveryClaim::Duplicate => return Ok(HandleOutcome::Duplicate),
            DeliveryClaim::Execute(evaluation_id) => evaluation_id,
        };
        let initial = adapter
            .refresh(&delivery)
            .map_err(ControllerError::Provider)?;
        validate_change(&delivery, &initial)?;
        let request = RunRequest {
            delivery: delivery.identity.clone(),
            provider_run: delivery.provider_run.clone(),
            evaluation_id: evaluation_id.clone(),
            run: initial.run.clone(),
        };
        let runner_outcome = match initial.state {
            ChangeState::Active => Some(self.runner.run(&request)),
            ChangeState::Superseded | ChangeState::Closed | ChangeState::AuthorizationRevoked => {
                None
            }
        };
        let fresh = adapter
            .refresh(&delivery)
            .map_err(ControllerError::Provider)?;
        validate_change(&delivery, &fresh)?;
        let publication = publication(&request, &initial, &fresh, runner_outcome);
        adapter
            .publish(&delivery, &publication)
            .map_err(ControllerError::Publish)?;
        self.ledger
            .complete(&delivery.identity, &evaluation_id)
            .map_err(ControllerError::Ledger)?;
        Ok(HandleOutcome::Published(publication.conclusion))
    }
}

fn validate_change<E>(
    delivery: &AuthenticatedDelivery,
    snapshot: &ChangeSnapshot,
) -> Result<(), ControllerError<E>> {
    if snapshot.run.change != delivery.change {
        return Err(ControllerError::WrongChangeIdentity);
    }
    if snapshot.run.object_format != delivery.provider_run.object_format
        || snapshot.run.commits.candidate != delivery.provider_run.candidate_commit
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
