#[cfg(unix)]
pub mod correlate;
#[cfg(unix)]
pub mod discovery;
pub mod document;
pub mod lfs;
#[cfg(unix)]
pub mod observe;
#[cfg(unix)]
pub mod resolve;
pub mod resources;
pub mod scan;

use amiss_md::Fault;
use amiss_wire::controls::ResourceName;
use amiss_wire::report::AnalysisErrorCode;

#[cfg(unix)]
pub use correlate::{Comparison, Impact, Observation, Outcome, Side, correlate};
#[cfg(unix)]
pub use discovery::{DocumentRecord, DocumentStatus, SnapshotDiscovery, UnsupportedKind, discover};
pub use document::{Classification, classify, excluded_by_built_in};
#[cfg(unix)]
pub use resolve::{GithubContext, Intent, Resolution, TargetCache, resolve};
pub use resources::{ScanLimits, ScanResources};
pub use scan::{Scanned, ScannedOccurrence, SpanDisplay, scan_bytes, scan_document};

pub const SOURCE_PROJECTION_DOMAIN: &str = "amiss/scanner-source-projection/v1";
pub const RAW_DESTINATION_DOMAIN: &str = "amiss/scanner-raw-destination/v1";

/// One failure: a parse fault in the contract's precedence, a Git acquisition
/// defect, a tree or index name outside `RepoPath`, or the first resource
/// crossing observed under the closed observation laws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Parse(Fault),
    Git(GitDefect),
    UnrepresentablePath,
    Internal,
    ResourceLimit {
        resource: ResourceName,
        configured_limit: u64,
        observed_lower_bound: u64,
    },
}

/// The disjoint Git lookup outcomes, minus resource crossings, which fold
/// into the shared crossing shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitDefect {
    RepositoryUnavailable,
    ObjectMissing,
    ObjectWrongKind,
    ObjectUnreadable,
}

impl From<amiss_git::Error> for Error {
    fn from(defect: amiss_git::Error) -> Self {
        match defect {
            amiss_git::Error::RepositoryUnavailable => Self::Git(GitDefect::RepositoryUnavailable),
            amiss_git::Error::ObjectMissing => Self::Git(GitDefect::ObjectMissing),
            amiss_git::Error::ObjectWrongKind => Self::Git(GitDefect::ObjectWrongKind),
            amiss_git::Error::ObjectUnreadable => Self::Git(GitDefect::ObjectUnreadable),
            amiss_git::Error::ResourceLimit {
                resource,
                configured_limit,
                observed_lower_bound,
            } => Self::ResourceLimit {
                resource,
                configured_limit,
                observed_lower_bound,
            },
        }
    }
}

impl Error {
    #[must_use]
    pub fn code(&self) -> AnalysisErrorCode {
        match self {
            Self::Parse(fault) => AnalysisErrorCode::from(*fault),
            Self::Git(GitDefect::RepositoryUnavailable) => {
                AnalysisErrorCode::GitRepositoryUnavailable
            }
            Self::Git(GitDefect::ObjectMissing) => AnalysisErrorCode::GitObjectMissing,
            Self::Git(GitDefect::ObjectWrongKind) => AnalysisErrorCode::GitObjectWrongKind,
            Self::Git(GitDefect::ObjectUnreadable) => AnalysisErrorCode::GitObjectUnreadable,
            Self::UnrepresentablePath => AnalysisErrorCode::UnrepresentablePath,
            Self::Internal => AnalysisErrorCode::InternalError,
            Self::ResourceLimit { .. } => AnalysisErrorCode::ResourceLimitExceeded,
        }
    }

    /// A crossing whose error row names the exact source document path fails
    /// that document alone; every other crossing exhausts a snapshot or
    /// evaluation budget and ends the stage.
    #[must_use]
    pub const fn is_document_scoped(&self) -> bool {
        match self {
            Self::Parse(_) | Self::Git(_) | Self::UnrepresentablePath => true,
            Self::Internal => false,
            Self::ResourceLimit { resource, .. } => matches!(
                resource,
                ResourceName::DocumentBlobBytes
                    | ResourceName::RawLinkDestinationBytes
                    | ResourceName::ParserNesting
                    | ResourceName::ParserNodesPerDocument
                    | ResourceName::ReferencesPerDocument
            ),
        }
    }
}
