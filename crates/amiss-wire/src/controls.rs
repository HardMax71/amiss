use std::cmp::Ordering;

use crate::codec::non_null;
use crate::de::{Error, ErrorKind, fail};
use crate::json::{self, Value};

pub use crate::semantic::RECORD_KEY_BYTES;

mod debt;
/// Execution-constraint descriptor, forge-neutral action-repository
/// identity, and closed platform grammar.
mod execution_constraint;
mod fact;
mod floor;
mod item;
mod mapping;
mod policy;
mod resources;
mod taxonomy;
/// Trusted-time statement grammar, digest, and bounded-lifetime parser.
mod trusted_time;
mod waiver;

pub use debt::{DebtItem, DebtSnapshot};
pub use execution_constraint::{
    ConstraintPlatform, ExecutionConstraintDescriptor, ExecutionConstraintInput,
    valid_required_status_name,
};
pub use fact::{Fact, FindingKeyInput, FindingScope, TargetIntent};
pub use floor::{
    FloorDefect, FloorDisposition, ORGANIZATION_POLICY_ENTRIES_LIMIT, OrganizationFloor,
    ResourceLimit,
};
pub(crate) use policy::compatible_source;
pub use policy::{
    BLOB_LINES_SOURCE, BlobLineSelection, DOCUMENT_SUFFIX_BYTES, DocumentInclude,
    FindingDisposition, NAMED_REGION_SOURCE, NamedRegionSelection, PREVIOUS_CODE_SINK,
    ProjectionAssertion, ProjectionKind, ProjectionSource, RECORD_SET_SOURCE, RECORD_VALUE_SOURCE,
    RecordSetSelection, RecordValueSelection, SOURCE_MARKER_BYTES, ScannerPolicy,
    TREE_PATHS_SOURCE, TreePathSelection, check_projection_source, document_include_value,
    parse_projection_source, projection_source_value,
};
pub use resources::{ResourceName, ResourceNameIter};
pub use taxonomy::{
    ContentAvailability, Disposition, EligibleFindingKind, EntryKind, GitMode, IncludeKind,
    Profile, PromotableFindingKind, SourceConstruct, TargetKind,
};
pub use trusted_time::{STATEMENT_TTL_MAX_SECONDS, TrustedTimeInput, TrustedTimeStatement};
pub use waiver::{WaiverBundle, WaiverItem};

pub const SCANNER_POLICY_PATH: &str = ".amiss/scanner-policy.json";

const SCANNER_POLICY_SCHEMA: &str = "amiss/scanner-policy";
const ORGANIZATION_FLOOR_SCHEMA: &str = "amiss/organization-floor";
const DEBT_SNAPSHOT_SCHEMA: &str = "amiss/debt-snapshot";
const WAIVER_BUNDLE_SCHEMA: &str = "amiss/waiver-bundle";

pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact";

/// The one restricted-JSON root every control document parses through.
///
/// # Errors
///
/// Any strict-JSON defect, carried as `ErrorKind::Json`.
pub fn root(bytes: &[u8]) -> Result<Value, Error> {
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

fn check_len(path: &str, length: usize, limit: usize) -> Result<(), Error> {
    if length > limit {
        fail(path, ErrorKind::LimitExceeded)
    } else {
        Ok(())
    }
}

fn check_schema(path: &str, actual: &str, expected: &str) -> Result<(), Error> {
    if actual == expected {
        Ok(())
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn sorted_set<T>(
    path: &str,
    items: &[T],
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), Error> {
    for pair in items.windows(2) {
        if let [left, right] = pair {
            match compare(left, right) {
                Ordering::Less => {}
                Ordering::Equal => return fail(path, ErrorKind::DuplicateMember),
                Ordering::Greater => return fail(path, ErrorKind::UnsortedSet),
            }
        }
    }
    Ok(())
}

/// Two resources fix their own maximum: the retained-error count is a small
/// range, and the report reservation may be declared but never moved.
fn in_bounds(resource: ResourceName, maximum: i64) -> bool {
    if resource == ResourceName::TypedAnalysisErrorsRetained {
        (1..=64).contains(&maximum)
    } else if resource == ResourceName::MachineJsonBytes {
        u64::try_from(maximum).is_ok_and(|value| value == crate::report::MACHINE_JSON_BYTES)
    } else {
        maximum >= 0
    }
}

pub(crate) fn valid_provider_run_id(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let allowed = |byte: &u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
    };
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(allowed)
}
