mod analysis;
mod build;
mod documents;
mod identity;
mod index;
mod summary;

pub(crate) use build::construct_with_site;
pub use build::{construct, construct_incomplete};
pub use identity::candidate_identity_digest;
pub use index::{INDEX_PROJECTION_SCHEMA, SNAPSHOT_SCHEMA, synthetic_candidate};

use amiss_wire::controls::{GitMode, Profile, ProjectionSource};
use amiss_wire::model::Digest;
use amiss_wire::model::{BranchRef, RepoPath};
use amiss_wire::report::model;
use amiss_wire::report::model::{ControlsUnavailableReason, SnapshotUnavailableReason};
use amiss_wire::report::{EngineProvenance, ErrorDetail};
pub use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
pub use amiss_wire::requests::GitSnapshotIdentity;
use amiss_wire::resolution::Resolution;

use crate::Error;

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";

/// The candidate side of the evaluation identity: a Git commit, the
/// synthetic complete logical staged index, or the unavailable projection an
/// incomplete index run reports with its closed reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateBlock {
    Commit(GitSnapshotIdentity),
    CommitUnavailable(Vec<SnapshotUnavailableReason>),
    Index(IndexCandidate),
    Unavailable(Vec<SnapshotUnavailableReason>),
}

/// The base side of a run: the commit it resolved to, or why it did not
/// resolve, since a commit ID is no tree ID and a report must not say so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseBlock {
    Commit(GitSnapshotIdentity),
    Unavailable(Vec<SnapshotUnavailableReason>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexCandidate {
    pub snapshot: amiss_wire::requests::IndexSnapshotIdentity,
    pub skip_worktree_paths: u64,
}

/// The diagnostic request digests of the wrapper lane: present exactly for
/// streams captured completely, and rendered only inside unavailable
/// snapshot and controls values. The in-process CLI has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestDigests {
    pub evaluation: Option<Digest>,
    pub snapshot: Option<Digest>,
    pub controls: Option<Digest>,
}

/// The run identity a complete local report carries, plus the acquired
/// policy effects and, for an invalid-policy run, the unavailable-controls
/// reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setup {
    pub engine: EngineProvenance,
    pub profile: Profile,
    pub repository: Option<amiss_wire::model::RepositoryIdentity>,
    pub forge: Option<amiss_wire::model::ForgeDialect>,
    pub candidate_ref: Option<BranchRef>,
    pub target_ref: Option<BranchRef>,
    pub default_branch_ref: Option<BranchRef>,
    pub base: BaseBlock,
    pub candidate: CandidateBlock,
    pub policy: crate::policy::Effects,
    pub controls_unavailable: Option<ControlsUnavailableReason>,
    pub requests: RequestDigests,
}

/// A constructed report: the envelope value, the payload spelled once in its
/// canonical form with the digest over it, and the result the process must
/// exit with. A binary streams the envelope around those bytes through its
/// reserved output buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built<
    P: amiss_wire::envelope::Payload<Schema = model::ReportEnvelopeSchema> = model::ReportPayload<
        RepoPath,
        Resolution<RepoPath>,
        GitMode,
        model::FindingFactEvidence<
            RepoPath,
            Resolution<RepoPath>,
            ProjectionSource,
            model::ProjectionDifference<Box<model::RowsProjectionDifference>>,
            GitMode,
        >,
    >,
> {
    pub envelope: model::ReportEnvelope<P>,
    pub payload_digest: Digest,
    pub canonical_payload: Vec<u8>,
    pub status: model::ReportStatus,
    pub exit_code: u8,
}

pub(crate) fn detail(error: &Error, path: Option<&RepoPath>) -> ErrorDetail {
    let resource = match error {
        Error::ResourceLimit {
            resource,
            configured_limit,
            observed_lower_bound,
        } => Some((*resource, *configured_limit, *observed_lower_bound)),
        Error::Parse(_) | Error::Git(_) | Error::UnrepresentablePath | Error::Internal => None,
    };
    ErrorDetail {
        code: error.code(),
        path: path.cloned(),
        path_bytes: None,
        resource,
    }
}

/// The exact canonical report bytes, including the trailing newline.
///
/// # Errors
/// Returns the serialization error without returning partial output.
pub fn wire(built: &Built) -> std::io::Result<Vec<u8>> {
    let mut wire = Vec::new();
    amiss_wire::report::emit_sealed(
        &built.envelope.schema,
        &built.canonical_payload,
        built.payload_digest,
        &mut wire,
    )?;
    Ok(wire)
}
