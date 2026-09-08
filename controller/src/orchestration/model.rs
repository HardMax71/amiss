use std::sync::Arc;
use std::time::Duration;

use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid};

use crate::{
    BoundSemanticEvidence, CapturedReport, ChangeLocator, CheckBinding, CheckPlan,
    ControllerEvaluationId, DeliveryIdentity, ProviderRunIdentity,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeState {
    Active,
    Superseded,
    Closed,
    AuthorizationRevoked,
}

/// The refs one run resolves against.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRefs {
    pub forge: ForgeDialect,
    pub candidate: BranchRef,
    pub target: BranchRef,
    pub default_branch: BranchRef,
}

/// One base and candidate pair of object ids.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidPair {
    pub base: Oid,
    pub candidate: Oid,
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
        let formats_match = [
            &commits.base,
            &commits.candidate,
            &trees.base,
            &trees.candidate,
        ]
        .into_iter()
        .all(|oid| oid.object_format() == object_format);
        formats_match.then_some(Self {
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
    /// Provider revision to which the adapter binds this run's gate.
    pub gate_commit: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRequest {
    pub delivery: DeliveryIdentity,
    pub provider_run: ProviderRunIdentity,
    pub evaluation_id: ControllerEvaluationId,
    pub check: CheckBinding,
    pub plan: Arc<CheckPlan>,
    pub run: RunIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evaluation {
    Pass,
    Block,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde_with::SerializeDisplay, strum::Display, strum::EnumIter,
)]
#[strum(serialize_all = "kebab-case")]
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
        report: Arc<CapturedReport>,
        semantic_artifact: Option<Arc<BoundSemanticEvidence>>,
    },
    MissingOutput,
    OversizedOutput,
    TimedOut,
    TamperedRuntime,
    Unavailable,
}

#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartbeatOutcome {
    /// Ownership is proven for this remaining window.
    Renewed { renew_within: Duration },
    /// Ownership cannot be proven and supervised work must terminate.
    Stop,
}

/// Cooperative lease renewal for one supervised run.
pub trait RunHeartbeat {
    /// Extends the live lease. A runner calls this before launch and again
    /// within every returned window. `Stop` means it must terminate and
    /// discard its output; the controller retains the exact failure.
    fn renew(&mut self) -> HeartbeatOutcome;
}

pub trait Runner {
    /// Runs the exact acquired identity. `Complete` is reserved for a report
    /// whose engine, exit class, and request bindings the trusted runner has
    /// already accepted; the controller independently rechecks the identity.
    /// Work starts only after a renewal and renews within every proven window;
    /// a `Stop` response terminates the run immediately.
    fn run(&mut self, request: &RunRequest, heartbeat: &mut dyn RunHeartbeat) -> RunnerOutcome;
}
