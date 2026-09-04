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

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::json::{Value, canonical};
use amiss_wire::model::{BranchRef, RepoPath};
use amiss_wire::report::EngineProvenance;
pub use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
pub use amiss_wire::requests::GitSnapshotIdentity as SnapshotIdentity;

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";

/// The candidate side of the evaluation identity: a Git commit, the
/// synthetic complete logical staged index, or the unavailable projection an
/// incomplete index run reports with its closed reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateBlock {
    Commit(SnapshotIdentity),
    Index(IndexCandidate),
    Unavailable(Vec<&'static str>),
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
    pub base: SnapshotIdentity,
    pub candidate: CandidateBlock,
    pub policy: crate::policy::Effects,
    pub controls_unavailable: Option<&'static str>,
    pub requests: RequestDigests,
}

/// A constructed report: the envelope value, the payload digest, and the
/// result the process must exit with. The wire is never materialized here;
/// a binary streams the envelope through its reserved fatal serializer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Built {
    pub envelope: Value,
    pub payload_digest: Digest,
    pub status: &'static str,
    pub exit_code: i64,
}

impl Built {
    /// The exact report wire, `JCS(envelope) || LF`, for callers that must
    /// hold the bytes.
    #[must_use]
    pub fn wire(&self) -> Vec<u8> {
        let mut wire = canonical(&self.envelope);
        wire.push(b'\n');
        wire
    }
}

fn string(text: &str) -> Value {
    Value::string(text.to_owned())
}

fn nullable(text: Option<&str>) -> Value {
    text.map_or(Value::Null, string)
}

fn nullable_path(path: Option<&RepoPath>) -> Value {
    path.map_or(Value::Null, RepoPath::to_value)
}

fn integer(value: u64) -> Value {
    Value::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

fn digest_value(digest: Digest) -> Value {
    Value::string(digest.to_string())
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}
