mod analysis;
mod build;
mod documents;
mod identity;
mod summary;

pub(crate) use build::construct_with_site;
pub use build::{construct, construct_incomplete};
pub use identity::candidate_identity_digest;

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::json::{Value, canonical};
use amiss_wire::model::RepoPath;
use amiss_wire::report::EngineProvenance;
pub use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
use amiss_wire::{codec, de::Error};
use serde::Serialize;

pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";
pub const INDEX_PROJECTION_SCHEMA: &str = "amiss/scanner-index-projection";
pub const SNAPSHOT_SCHEMA: &str = "amiss/scanner-snapshot";

#[derive(Serialize)]
struct IndexEntry<'a> {
    entry_kind: &'static str,
    git_mode: amiss_wire::controls::GitMode,
    object_format: &'static str,
    object_oid: &'a str,
    path: &'a RepoPath,
    skip_worktree: bool,
}

#[derive(Serialize)]
struct IndexProjection<'a> {
    entries: Vec<IndexEntry<'a>>,
    schema: &'static str,
}

#[derive(Serialize)]
struct SnapshotInput<'a> {
    base_commit_oid: &'a str,
    base_object_format: &'static str,
    identity_scope: &'static str,
    index_projection_digest: Digest,
    kind: &'static str,
    schema: &'static str,
}

/// The logical-index projection and synthetic snapshot identity.
///
/// # Errors
///
/// The index cannot be represented in the strict JSON profile.
pub fn synthetic_candidate(
    base_object_format: &'static str,
    base_commit_oid: &str,
    entries: &[(RepoPath, amiss_wire::controls::GitMode, String, bool)],
    skip_worktree_paths: u64,
) -> Result<IndexCandidate, Error> {
    let entries = entries
        .iter()
        .map(|(path, mode, oid, skip)| {
            let entry_kind = match mode {
                amiss_wire::controls::GitMode::Symlink => "symlink",
                amiss_wire::controls::GitMode::Gitlink => "gitlink",
                amiss_wire::controls::GitMode::RegularFile
                | amiss_wire::controls::GitMode::ExecutableFile
                | amiss_wire::controls::GitMode::Tree => "blob",
            };
            IndexEntry {
                entry_kind,
                git_mode: *mode,
                object_format: base_object_format,
                object_oid: oid,
                path,
                skip_worktree: *skip,
            }
        })
        .collect();
    let projection = IndexProjection {
        entries,
        schema: INDEX_PROJECTION_SCHEMA,
    };
    let projection_digest = codec::digest(INDEX_PROJECTION_SCHEMA, &projection)?;
    let snapshot_digest = codec::digest(
        SNAPSHOT_SCHEMA,
        &SnapshotInput {
            base_commit_oid,
            base_object_format,
            identity_scope: "complete-logical-index",
            index_projection_digest: projection_digest,
            kind: "index",
            schema: SNAPSHOT_SCHEMA,
        },
    )?;
    Ok(IndexCandidate {
        base_object_format,
        base_commit_oid: base_commit_oid.to_owned(),
        projection_digest,
        entry_count: u64::try_from(projection.entries.len()).unwrap_or(u64::MAX),
        snapshot_digest,
        skip_worktree_paths,
    })
}

/// One snapshot's identity in the evaluation block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotIdentity {
    pub object_format: &'static str,
    pub commit_oid: String,
    pub tree_oid: String,
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
    pub base_object_format: &'static str,
    pub base_commit_oid: String,
    pub projection_digest: Digest,
    pub entry_count: u64,
    pub snapshot_digest: Digest,
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
    pub candidate_ref: Option<String>,
    pub target_ref: Option<String>,
    pub default_branch_ref: Option<String>,
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
