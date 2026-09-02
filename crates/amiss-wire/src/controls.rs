use std::cmp::Ordering;
use std::str::FromStr;

use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::json::{self, Value};
use crate::model::{
    ArtifactId, BranchRef, OwnerId, RepoPathText, RepositoryIdentity, TreeIdentity, UtcInstant,
};

pub use crate::semantic::RECORD_KEY_BYTES;

mod debt;
/// Execution-constraint descriptor, forge-neutral action-repository
/// identity, and closed platform grammar.
mod execution_constraint;
mod fact;
mod floor;
mod item;
mod policy;
mod resources;
mod taxonomy;
/// Trusted-time statement grammar, digest, and bounded-lifetime parser.
mod trusted_time;
pub(crate) mod value;
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
pub use trusted_time::{
    STATEMENT_TTL_MAX_SECONDS, TRUSTED_TIME_CONTROLLER, TRUSTED_TIME_STATEMENT_SCHEMA,
    TrustedTimeController, TrustedTimeSchema, TrustedTimeStatement, canonical_trusted_time,
    parse_trusted_time,
};
pub use waiver::{WaiverBundle, WaiverItem};

pub const SCANNER_POLICY_PATH: &str = ".amiss/scanner-policy.json";

const SCANNER_POLICY_SCHEMA: &str = "amiss/scanner-policy";
const ORGANIZATION_FLOOR_SCHEMA: &str = "amiss/organization-floor";
const DEBT_SNAPSHOT_SCHEMA: &str = "amiss/debt-snapshot";
const WAIVER_BUNDLE_SCHEMA: &str = "amiss/waiver-bundle";

pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact";

pub(crate) fn decode_enum<T: FromStr>(path: &str, value: Value) -> Result<T, Error> {
    let raw = de::string(path, value)?;
    raw.parse()
        .map_err(|_unknown| Error::new(path, ErrorKind::InvalidValue))
}

/// The one restricted-JSON root every control document parses through.
///
/// # Errors
///
/// Any strict-JSON defect, carried as `ErrorKind::Json`.
pub fn root(bytes: &[u8]) -> Result<Value, Error> {
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

fn decode_items<T>(
    path: &str,
    raw: Vec<Value>,
    limit: usize,
    decode: impl Fn(&str, Value) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    if raw.len() > limit {
        return fail(path, ErrorKind::LimitExceeded);
    }
    raw.into_iter()
        .enumerate()
        .map(|(index, value)| decode(&format!("{path}[{index}]"), value))
        .collect()
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

fn decode_disposition_rule(path: &str, value: Value) -> Result<FindingDisposition, Error> {
    let mut obj = Obj::new(path, value)?;
    let finding_kind = obj.required("finding_kind", decode_enum)?;
    let disposition = obj.required("disposition", decode_enum)?;
    obj.finish()?;
    Ok(FindingDisposition {
        finding_kind,
        disposition,
    })
}

fn decode_resource_limit(path: &str, value: Value) -> Result<ResourceLimit, Error> {
    let mut obj = Obj::new(path, value)?;
    let resource = obj.required("resource", ResourceName::decode)?;
    let maximum_path = obj.field("maximum");
    let maximum = de::integer(&maximum_path, obj.take("maximum")?)?;
    obj.finish()?;
    if in_bounds(resource, maximum) {
        Ok(ResourceLimit { resource, maximum })
    } else {
        fail(&maximum_path, ErrorKind::InvalidValue)
    }
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

fn decode_path_set(path: &str, value: Value) -> Result<Vec<RepoPathText>, Error> {
    decode_path_items(path, de::array(path, value)?)
}

fn decode_path_items(path: &str, raw: Vec<Value>) -> Result<Vec<RepoPathText>, Error> {
    let paths = decode_items(path, raw, 100_000, decode_repo_path)?;
    sorted_set(path, &paths, |a, b| a.as_str().cmp(b.as_str()))?;
    Ok(paths)
}

fn decode_owner_items(path: &str, raw: Vec<Value>) -> Result<Vec<OwnerId>, Error> {
    let owners = decode_items(path, raw, 10_000, decode_owner)?;
    sorted_set(path, &owners, |a, b| a.as_str().cmp(b.as_str()))?;
    Ok(owners)
}

fn decode_repo_path(path: &str, value: Value) -> Result<RepoPathText, Error> {
    RepoPathText::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_artifact_id(path: &str, value: Value) -> Result<ArtifactId, Error> {
    ArtifactId::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_owner(path: &str, value: Value) -> Result<OwnerId, Error> {
    OwnerId::new(de::string(path, value)?).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_branch_ref(path: &str, value: Value) -> Result<BranchRef, Error> {
    BranchRef::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_instant(path: &str, value: Value) -> Result<UtcInstant, Error> {
    UtcInstant::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(crate) fn decode_repository(path: &str, value: Value) -> Result<RepositoryIdentity, Error> {
    let mut obj = Obj::new(path, value)?;
    let host = obj.required("host", de::string)?;
    let owner = obj.required("owner", de::string)?;
    let name = obj.required("name", de::string)?;
    obj.finish()?;
    RepositoryIdentity::new(host, owner, name)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(crate) fn provider_run_id_valid(raw: &str) -> bool {
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

fn decode_tree(path: &str, value: Value) -> Result<TreeIdentity, Error> {
    let mut obj = Obj::new(path, value)?;
    let object_format = obj.required("object_format", decode_enum)?;
    let tree_oid = obj.required("tree_oid", de::string)?;
    obj.finish()?;
    TreeIdentity::new(object_format, tree_oid)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}
