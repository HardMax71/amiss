use amiss_wire::controls::GitMode;
use amiss_wire::digest::{Digest, hj_serde};
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::model::DocumentEntryKind;
use amiss_wire::requests::{
    IndexIdentityScope, IndexSnapshotIdentity, IndexSnapshotKind, IndexSnapshotSchema,
};
use serde::Serialize;

use super::IndexCandidate;

pub const INDEX_PROJECTION_SCHEMA: &str = "amiss/scanner-index-projection";
pub const SNAPSHOT_SCHEMA: &str = "amiss/scanner-snapshot";

#[derive(Serialize)]
struct IndexEntry<'a> {
    entry_kind: DocumentEntryKind,
    git_mode: GitMode,
    object_format: ObjectFormat,
    object_oid: &'a Oid,
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
    base_commit_oid: &'a Oid,
    base_object_format: ObjectFormat,
    identity_scope: IndexIdentityScope,
    index_projection_digest: Digest,
    kind: IndexSnapshotKind,
    schema: IndexSnapshotSchema,
}

/// The logical-index projection and synthetic snapshot built over it, with both digests.
///
/// # Errors
///
/// Returns [`crate::Error::Internal`] if an identity cannot be serialized.
pub fn synthetic_candidate(
    base_object_format: ObjectFormat,
    base_commit_oid: &Oid,
    entries: &[(RepoPath, GitMode, Oid, bool)],
    skip_worktree_paths: u64,
) -> Result<IndexCandidate, crate::Error> {
    let projection = IndexProjection {
        entries: entries
            .iter()
            .map(|(path, mode, oid, skip)| IndexEntry {
                entry_kind: match mode {
                    GitMode::Symlink => DocumentEntryKind::Symlink,
                    GitMode::Gitlink => DocumentEntryKind::Gitlink,
                    GitMode::RegularFile | GitMode::ExecutableFile | GitMode::Tree => {
                        DocumentEntryKind::Blob
                    }
                },
                git_mode: *mode,
                object_format: base_object_format,
                object_oid: oid,
                path,
                skip_worktree: *skip,
            })
            .collect(),
        schema: INDEX_PROJECTION_SCHEMA,
    };
    let index_projection_digest = hj_serde(INDEX_PROJECTION_SCHEMA, |writer| {
        serde_json::to_writer(writer, &projection)
    })
    .map_err(|_defect| crate::Error::Internal)?;
    let input = SnapshotInput {
        base_commit_oid,
        base_object_format,
        identity_scope: IndexIdentityScope::CompleteLogicalIndex,
        index_projection_digest,
        kind: IndexSnapshotKind::Index,
        schema: IndexSnapshotSchema::Current,
    };
    let snapshot_digest = hj_serde(SNAPSHOT_SCHEMA, |writer| {
        serde_json::to_writer(writer, &input)
    })
    .map_err(|_defect| crate::Error::Internal)?;
    Ok(IndexCandidate {
        snapshot: IndexSnapshotIdentity {
            base_commit_oid: base_commit_oid.clone(),
            base_object_format,
            entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
            identity_scope: input.identity_scope,
            index_projection_digest,
            kind: input.kind,
            snapshot_digest,
            snapshot_schema: input.schema,
        },
        skip_worktree_paths,
    })
}
