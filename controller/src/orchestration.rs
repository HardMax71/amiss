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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunIdentity {
    change: ChangeLocator,
    forge: ForgeDialect,
    target_ref: BranchRef,
    candidate_ref: BranchRef,
    default_branch_ref: BranchRef,
    object_format: ObjectFormat,
    base_commit: Oid,
    candidate_commit: Oid,
    base_tree: Oid,
    candidate_tree: Oid,
}

impl RunIdentity {
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor makes every exact run-binding field explicit"
    )]
    pub fn new(
        change: ChangeLocator,
        forge: ForgeDialect,
        target_ref: BranchRef,
        candidate_ref: BranchRef,
        default_branch_ref: BranchRef,
        object_format: ObjectFormat,
        base_commit: Oid,
        candidate_commit: Oid,
        base_tree: Oid,
        candidate_tree: Oid,
    ) -> Option<Self> {
        let valid = [&base_commit, &candidate_commit, &base_tree, &candidate_tree]
            .into_iter()
            .all(|oid| Oid::new(object_format, oid.as_str().to_owned()).is_some());
        if !valid {
            return None;
        }
        Some(Self {
            change,
            forge,
            target_ref,
            candidate_ref,
            default_branch_ref,
            object_format,
            base_commit,
            candidate_commit,
            base_tree,
            candidate_tree,
        })
    }

    #[must_use]
    pub const fn change(&self) -> &ChangeLocator {
        &self.change
    }

    #[must_use]
    pub const fn forge(&self) -> ForgeDialect {
        self.forge
    }

    #[must_use]
    pub const fn target_ref(&self) -> &BranchRef {
        &self.target_ref
    }

    #[must_use]
    pub const fn candidate_ref(&self) -> &BranchRef {
        &self.candidate_ref
    }

    #[must_use]
    pub const fn default_branch_ref(&self) -> &BranchRef {
        &self.default_branch_ref
    }

    #[must_use]
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    #[must_use]
    pub const fn base_commit(&self) -> &Oid {
        &self.base_commit
    }

    #[must_use]
    pub const fn candidate_commit(&self) -> &Oid {
        &self.candidate_commit
    }

    #[must_use]
    pub const fn base_tree(&self) -> &Oid {
        &self.base_tree
    }

    #[must_use]
    pub const fn candidate_tree(&self) -> &Oid {
        &self.candidate_tree
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
    registry: AdapterRegistry,
    ledger: L,
    runner: R,
}

impl<L, R> Controller<L, R>
where
    L: DeliveryLedger,
    R: Runner,
{
    #[must_use]
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

    #[must_use]
    pub const fn ledger(&self) -> &L {
        &self.ledger
    }

    #[must_use]
    pub const fn runner(&self) -> &R {
        &self.runner
    }
}

fn validate_change<E>(
    delivery: &AuthenticatedDelivery,
    snapshot: &ChangeSnapshot,
) -> Result<(), ControllerError<E>> {
    if snapshot.run.change() != &delivery.change {
        return Err(ControllerError::WrongChangeIdentity);
    }
    if snapshot.run.object_format() != delivery.provider_run.object_format()
        || snapshot.run.candidate_commit() != delivery.provider_run.candidate_commit()
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
        Some(RunnerOutcome::Complete {
            identity,
            evaluation: _,
            report: _,
        }) if identity_without_trees(&identity) != identity_without_trees(expected) => (
            CheckConclusion::Unavailable(RunFailure::WrongIdentity),
            None,
        ),
        Some(RunnerOutcome::Complete { identity, .. })
            if identity.base_tree() != expected.base_tree()
                || identity.candidate_tree() != expected.candidate_tree() =>
        {
            (CheckConclusion::Unavailable(RunFailure::WrongTree), None)
        }
        Some(RunnerOutcome::Complete {
            evaluation: _,
            report,
            ..
        }) if report.is_empty() => (
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

fn identity_without_trees(
    identity: &RunIdentity,
) -> (
    &ChangeLocator,
    ForgeDialect,
    &BranchRef,
    &BranchRef,
    &BranchRef,
    ObjectFormat,
    &Oid,
    &Oid,
) {
    (
        identity.change(),
        identity.forge(),
        identity.target_ref(),
        identity.candidate_ref(),
        identity.default_branch_ref(),
        identity.object_format(),
        identity.base_commit(),
        identity.candidate_commit(),
    )
}
