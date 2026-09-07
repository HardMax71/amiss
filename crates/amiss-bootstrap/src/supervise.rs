use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::model::{Oid, RepositoryIdentity};
use amiss_wire::report::model::SemanticEvidenceProvenance;
use amiss_wire::requests::RequestTrust;

mod controls;
mod identity;
mod model;
mod read;

pub use read::accept;

/// The exact acceptance defect, most specific first in evaluation order. The
/// trusted wrapper publishes success only when acceptance returns no defect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptanceDefect {
    /// The bytes are not one parsable envelope with the expected members.
    Shape,
    /// The bytes are not exactly `JCS(envelope) || LF`.
    Noncanonical,
    /// The payload-only digest does not recompute.
    PayloadDigest,
    /// The engine digest differs from the binary the wrapper validated.
    Engine,
    /// The evaluated base identity differs from the one requested.
    BaseIdentity,
    /// The evaluated candidate identity differs from the one requested.
    CandidateIdentity,
    /// The report does not bind the sealed refs and candidate identity.
    SealedIdentity,
    /// The report does not carry the exact sealed controls and provider run.
    SealedControls,
    /// The status, completeness flag and exit class disagree.
    Completeness,
    /// The finding count differs from the findings array length.
    FindingCount,
}

/// What the wrapper expects the accepted envelope to carry: the digest of the
/// binary it validated and launched, and the identities it asked that binary
/// to evaluate. A wrapper can only hold an engine to what it knows it
/// requested.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expectations {
    pub engine_digest: Digest,
    pub base_commit: Oid,
    pub candidate_commit: Option<Oid>,
    pub sealed: Option<SealedExpectations>,
}

/// The independently captured provider and control context a sealed report
/// must reproduce before the bootstrap will publish it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedExpectations {
    pub profile: Profile,
    pub candidate_ref: String,
    pub target_ref: String,
    pub repository: RepositoryIdentity,
    pub provider: String,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
    pub candidate_identity_digest: Digest,
    pub organization_floor: Option<SealedControlExpectation>,
    pub debt_snapshot: Option<SealedControlExpectation>,
    pub waiver_bundle: Option<SealedControlExpectation>,
    pub execution_constraint: SealedControlExpectation,
    pub trusted_time_digest: Digest,
    pub semantic_evidence: Vec<SemanticEvidenceProvenance>,
}

/// One exact externally authenticated control projection expected in the
/// report after the engine verifies its embedded semantic digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedControlExpectation {
    pub digest: Digest,
    pub trust_source: RequestTrust,
}

/// The watchdog outcome for one spawned engine process.
#[derive(Debug)]
pub enum Supervised {
    /// The engine exited on its own within the ceiling.
    Completed(ExitStatus),
    /// The ceiling passed; the engine was killed and reaped. A killed engine
    /// yields no accepted envelope.
    Killed,
}

/// The operational wall-time watchdog: polls the engine until it exits or the
/// ceiling passes, then kills and reaps it. The kill can never produce a
/// partial result whose presence depends on runner speed; the caller fails the
/// run without an envelope.
///
/// # Errors
///
/// Only `try_wait` failures; kill and reap errors after a timeout are
/// deliberately ignored because the outcome is already `Killed`.
pub fn supervise(child: &mut Child, ceiling: Duration) -> std::io::Result<Supervised> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Supervised::Completed(status));
        }
        if start.elapsed() >= ceiling {
            let _signalled = child.kill();
            let _reaped = child.wait();
            return Ok(Supervised::Killed);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Why a run produced no accepted result. Every one of these is a failed
/// required check, and none of them publishes an envelope: a report the
/// wrapper cannot accept is not a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Defect {
    /// The engine outlived the wall ceiling and was killed.
    Killed,
    /// The engine died on a signal and carries no exit code.
    Signalled,
    /// The engine wrote more than the wire ceiling admits.
    Oversize,
    /// The engine's own exit code disagrees with the exit class it reported.
    ExitMismatch,
    /// The envelope failed the acceptance law.
    Acceptance(AcceptanceDefect),
}

/// The settlement law, over what the wrapper can observe of a finished engine:
/// its exit code and its complete stdout. An accepted envelope returns the
/// exit class the wrapper then exits with, and which the engine's own process
/// exit code must already equal. Nothing else is publishable.
///
/// # Errors
///
/// The defect that refused the result.
pub fn settle(
    outcome: &Supervised,
    stdout: &[u8],
    expectations: &Expectations,
) -> Result<i64, Defect> {
    let status = match *outcome {
        Supervised::Killed => return Err(Defect::Killed),
        Supervised::Completed(status) => status,
    };
    if u64::try_from(stdout.len()).unwrap_or(u64::MAX) > amiss_wire::report::MACHINE_JSON_BYTES {
        return Err(Defect::Oversize);
    }
    let code = status.code().ok_or(Defect::Signalled)?;
    let class = accept(stdout, expectations).map_err(Defect::Acceptance)?;
    if i64::from(code) != class {
        return Err(Defect::ExitMismatch);
    }
    Ok(class)
}
