use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};

use crate::{ChangeLocator, ControllerEvaluationId, DeliveryIdentity, ProviderRunIdentity};

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

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartbeatOutcome {
    /// Ownership is proven through this advisory deadline.
    Renewed { expires_at_unix_millis: i64 },
    /// Ownership cannot be proven and supervised work must terminate.
    Stop,
}

/// Cooperative lease renewal for one supervised run.
pub trait RunHeartbeat {
    /// The current advisory deadline; only a ledger renewal decides ownership.
    fn expires_at_unix_millis(&self) -> i64;

    /// Extends the live lease. The returned deadline is the only proven window;
    /// `Stop` means the runner must terminate and discard its output. The
    /// controller retains the exact failure.
    fn renew(&mut self) -> HeartbeatOutcome;
}

pub trait Runner {
    /// Runs the exact acquired identity. `Complete` is reserved for a report
    /// whose engine, exit class, and request bindings the trusted runner has
    /// already accepted; the controller independently rechecks the identity.
    /// Work that may cross the heartbeat deadline must renew before it does;
    /// a `Stop` response terminates the run immediately.
    fn run(&mut self, request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome;
}
