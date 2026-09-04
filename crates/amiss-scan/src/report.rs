mod analysis;
mod build;
mod documents;
mod identity;
mod summary;

pub(crate) use build::construct_with_site;
pub use build::{construct, construct_incomplete};
pub use identity::candidate_identity_digest;

use amiss_wire::controls::Profile;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::{Value, canonical};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::EngineProvenance;
pub use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
pub use amiss_wire::requests::GitSnapshotIdentity as SnapshotIdentity;

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";
pub const INDEX_PROJECTION_SCHEMA: &str = "amiss/scanner-index-projection";
pub const SNAPSHOT_SCHEMA: &str = "amiss/scanner-snapshot";

/// The canonical logical-index projection and the synthetic snapshot input
/// built over it, with both digests.
#[must_use]
pub fn synthetic_candidate(
    base_object_format: ObjectFormat,
    base_commit_oid: &Oid,
    entries: &[(RepoPath, amiss_wire::controls::GitMode, Oid, bool)],
    skip_worktree_paths: u64,
) -> IndexCandidate {
    let rows: Vec<Value> = entries
        .iter()
        .map(|(path, mode, oid, skip)| {
            let entry_kind = match mode {
                amiss_wire::controls::GitMode::Symlink => "symlink",
                amiss_wire::controls::GitMode::Gitlink => "gitlink",
                amiss_wire::controls::GitMode::RegularFile
                | amiss_wire::controls::GitMode::ExecutableFile
                | amiss_wire::controls::GitMode::Tree => "blob",
            };
            object(vec![
                ("path", path.to_value()),
                ("entry_kind", string(entry_kind)),
                ("git_mode", string(mode.as_ref())),
                ("object_format", string(base_object_format.as_ref())),
                ("object_oid", string(oid.as_str())),
                ("skip_worktree", Value::Bool(*skip)),
            ])
        })
        .collect();
    let projection = object(vec![
        ("schema", string(INDEX_PROJECTION_SCHEMA)),
        ("entries", Value::array(rows)),
    ]);
    let projection_digest = hj(INDEX_PROJECTION_SCHEMA, &projection);
    let snapshot_input = object(vec![
        ("schema", string(SNAPSHOT_SCHEMA)),
        ("kind", string("index")),
        ("identity_scope", string("complete-logical-index")),
        ("base_object_format", string(base_object_format.as_ref())),
        ("base_commit_oid", string(base_commit_oid.as_str())),
        ("index_projection_digest", digest_value(projection_digest)),
    ]);
    let snapshot_digest = hj(SNAPSHOT_SCHEMA, &snapshot_input);
    IndexCandidate {
        snapshot: amiss_wire::requests::IndexSnapshotIdentity {
            base_commit_oid: base_commit_oid.clone(),
            base_object_format,
            entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
            identity_scope: amiss_wire::requests::IndexIdentityScope::CompleteLogicalIndex,
            index_projection_digest: projection_digest,
            kind: amiss_wire::requests::IndexSnapshotKind::Index,
            snapshot_digest,
            snapshot_schema: amiss_wire::requests::IndexSnapshotSchema::Current,
        },
        skip_worktree_paths,
    }
}

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
