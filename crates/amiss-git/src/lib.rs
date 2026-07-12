pub mod object;
#[cfg(unix)]
mod pack;
#[cfg(unix)]
pub mod repo;
pub mod resources;

pub use object::{Commit, Object, ObjectKind, TreeEntry, parse_commit, parse_tree};
#[cfg(unix)]
pub use repo::Repository;
pub use resources::{GitLimits, GitResources};

use amiss_wire::controls::ResourceName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    RepositoryUnavailable,
    ObjectMissing,
    ObjectWrongKind,
    ObjectUnreadable,
    ResourceLimit {
        resource: ResourceName,
        configured_limit: u64,
        observed_lower_bound: u64,
    },
}
