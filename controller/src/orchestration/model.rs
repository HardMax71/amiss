use std::time::Duration;

use amiss_wire::model::ObjectFormat;

use crate::{ChangeLocator, OidPair, RunIdentity, RunRefs, RunRequest};

impl OidPair {
    pub(crate) fn well_formed(&self, object_format: ObjectFormat) -> bool {
        [&self.base, &self.candidate]
            .into_iter()
            .all(|oid| oid.object_format() == object_format)
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evaluation {
    Pass,
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunnerOutcome {
    Complete {
        identity: Box<RunIdentity>,
        evaluation: Evaluation,
        report: Vec<u8>,
        semantic_artifact: Option<Vec<u8>>,
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
