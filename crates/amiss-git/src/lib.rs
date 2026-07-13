mod handle;
pub mod index;
pub mod object;
mod pack;
pub mod repo;
pub mod resources;

pub use index::{IndexEntry, LogicalIndex, parse_index_file};
pub use object::{Commit, Object, ObjectKind, TreeEntry, parse_commit, parse_tree};
pub use repo::Repository;
pub use resources::{GitLimits, GitResources, ValueCap};

use amiss_wire::controls::ResourceName;

/// The spec requires that a platform which cannot enforce the handle/no-follow
/// boundary report the repository unavailable rather than fall back to
/// pathname traversal. Unix and Windows both enforce it, so the crate refuses
/// to build anywhere else instead of shipping a fallback that could be reached
/// by accident. A build that cannot hold the boundary does not exist.
#[cfg(not(any(unix, windows)))]
compile_error!(
    "amiss-git holds every object behind a directory handle that is never followed; no other platform provides one"
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    RepositoryUnavailable,
    ObjectMissing,
    ObjectWrongKind,
    ObjectUnreadable,
    IndexInvalid,
    IndexUnmerged,
    IntentToAdd,
    SnapshotChanged,
    ResourceLimit {
        resource: ResourceName,
        configured_limit: u64,
        observed_lower_bound: u64,
    },
}
