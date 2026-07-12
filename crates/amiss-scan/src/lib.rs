pub mod document;
pub mod resources;
pub mod scan;

use amiss_md::Fault;
use amiss_wire::controls::ResourceName;
use amiss_wire::report::AnalysisErrorCode;

pub use document::{Classification, classify, excluded_by_built_in};
pub use resources::{ScanLimits, ScanResources};
pub use scan::{Scanned, ScannedOccurrence, SpanDisplay, scan_document};

pub const SOURCE_PROJECTION_DOMAIN: &str = "amiss/scanner-source-projection/v1";
pub const RAW_DESTINATION_DOMAIN: &str = "amiss/scanner-raw-destination/v1";

/// One document's failure: a parse fault in the contract's precedence, or the
/// first resource crossing, observed under the closed observation laws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Parse(Fault),
    ResourceLimit {
        resource: ResourceName,
        configured_limit: u64,
        observed_lower_bound: u64,
    },
}

impl Error {
    #[must_use]
    pub fn code(&self) -> AnalysisErrorCode {
        match self {
            Self::Parse(fault) => AnalysisErrorCode::from(*fault),
            Self::ResourceLimit { .. } => AnalysisErrorCode::ResourceLimitExceeded,
        }
    }
}
