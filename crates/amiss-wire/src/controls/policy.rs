use serde::{Deserialize, Serialize};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::extraction::governed_name_valid;
use crate::json::{self, Value};
use crate::model::{Adapter, ArtifactId, RepoPathText};

use super::{
    Disposition, IncludeKind, PromotableFindingKind, SCANNER_POLICY_SCHEMA, root, sorted_set,
};

/// Maximum UTF-8 byte length of one exact document suffix selector.
pub const DOCUMENT_SUFFIX_BYTES: usize = 64;
pub const PREVIOUS_CODE_SINK: &str = "previous-code";
pub const BLOB_LINES_SOURCE: &str = "blob-lines";
pub const NAMED_REGION_SOURCE: &str = "named-region";
pub const TREE_PATHS_SOURCE: &str = "tree-paths";
pub const RECORD_VALUE_SOURCE: &str = "record-value";
pub const RECORD_SET_SOURCE: &str = "record-set";
pub const SOURCE_MARKER_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScannerPolicySchema {
    #[serde(rename = "amiss/scanner-policy")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr, strum::EnumString, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectionKind {
    #[strum(serialize = "code-text-v1")]
    CodeTextV1,
    #[strum(serialize = "sorted-rows-v1")]
    SortedRowsV1,
    #[strum(serialize = "decimal-count-v1")]
    DecimalCountV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectionSink {
    #[serde(rename = "previous-code")]
    PreviousCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentInclude {
    pub path: RepoPathText,
    pub kind: IncludeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<Adapter>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingDisposition {
    pub finding_kind: PromotableFindingKind,
    pub disposition: Disposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobLineSelection {
    pub path: RepoPathText,
    pub first_line: u64,
    pub last_line: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedRegionSelection {
    pub path: RepoPathText,
    pub start_marker: String,
    pub end_marker: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreePathSelection {
    pub root: RepoPathText,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    pub maximum_depth: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordValueSelection {
    pub set: ArtifactId,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordSetSelection {
    pub set: ArtifactId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ProjectionSource {
    BlobLines(BlobLineSelection),
    NamedRegion(NamedRegionSelection),
    TreePaths(TreePathSelection),
    RecordValue(RecordValueSelection),
    RecordSet(RecordSetSelection),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionAssertion {
    pub document: RepoPathText,
    pub name: String,
    pub projection: ProjectionKind,
    pub sink: ProjectionSink,
    pub source: ProjectionSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScannerPolicy {
    pub schema: ScannerPolicySchema,
    pub document_includes: Vec<DocumentInclude>,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub projection_assertions: Option<Vec<ProjectionAssertion>>,
    pub protected_inventory: Vec<RepoPathText>,
    pub finding_dispositions: Vec<FindingDisposition>,
}

/// Parses and validates one repository scanner policy.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, unknown fields,
/// invalid grammar values, and unsorted or duplicate set members.
pub fn parse_scanner_policy(bytes: &[u8]) -> Result<ScannerPolicy, Error> {
    root(bytes)?;
    let policy = de::deserialize_json(bytes)?;
    validate_scanner_policy(&policy)?;
    Ok(policy)
}

/// Produces one valid scanner policy's canonical bytes and digest.
///
/// # Errors
///
/// A public field violates the same laws [`parse_scanner_policy`] enforces,
/// or the typed value cannot be serialized.
pub fn canonical_scanner_policy(policy: &ScannerPolicy) -> Result<(Vec<u8>, Digest), Error> {
    validate_scanner_policy(policy)?;
    let bytes = serde_json_canonicalizer::to_vec(policy)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(SCANNER_POLICY_SCHEMA, &bytes);
    Ok((bytes, digest))
}

/// Checks a directly constructed source through the same closed grammar and
/// projection compatibility laws as a scanner-policy assertion.
///
/// # Errors
///
/// The source violates its bounded field grammar or is incompatible with the
/// selected projection.
pub fn check_projection_source(
    projection: ProjectionKind,
    source: &ProjectionSource,
) -> Result<(), Error> {
    validate_projection_source("$", projection, source)
}

/// Parses one standalone projection source through the scanner-policy grammar.
///
/// # Errors
///
/// The JSON is not strict, the source is malformed, or it is incompatible with the selected
/// projection.
pub fn parse_projection_source(
    bytes: &[u8],
    projection: ProjectionKind,
) -> Result<ProjectionSource, Error> {
    root(bytes)?;
    let source = de::deserialize_json(bytes)?;
    validate_projection_source("$", projection, &source)?;
    Ok(source)
}

fn validate_scanner_policy(policy: &ScannerPolicy) -> Result<(), Error> {
    if policy.document_includes.len() > 100_000 {
        return fail("$.document_includes", ErrorKind::LimitExceeded);
    }
    for (index, include) in policy.document_includes.iter().enumerate() {
        validate_document_include(&format!("$.document_includes[{index}]"), include)?;
    }
    sorted_set(
        "$.document_includes",
        &policy.document_includes,
        |left, right| (left.path.as_str(), left.kind).cmp(&(right.path.as_str(), right.kind)),
    )?;

    let assertions = policy.projection_assertions.as_deref().unwrap_or_default();
    if assertions.len() > 100_000 {
        return fail("$.projection_assertions", ErrorKind::LimitExceeded);
    }
    for (index, assertion) in assertions.iter().enumerate() {
        validate_projection_assertion(&format!("$.projection_assertions[{index}]"), assertion)?;
    }
    sorted_set("$.projection_assertions", assertions, |left, right| {
        (left.document.as_str(), left.name.as_str())
            .cmp(&(right.document.as_str(), right.name.as_str()))
    })?;

    if policy.protected_inventory.len() > 100_000 {
        return fail("$.protected_inventory", ErrorKind::LimitExceeded);
    }
    sorted_set(
        "$.protected_inventory",
        &policy.protected_inventory,
        |left, right| left.as_str().cmp(right.as_str()),
    )?;

    if policy.finding_dispositions.len() > 3 {
        return fail("$.finding_dispositions", ErrorKind::LimitExceeded);
    }
    sorted_set(
        "$.finding_dispositions",
        &policy.finding_dispositions,
        |left, right| left.finding_kind.as_ref().cmp(right.finding_kind.as_ref()),
    )
}

fn validate_document_include(path: &str, include: &DocumentInclude) -> Result<(), Error> {
    if include.suffix.is_some() && include.kind != IncludeKind::Tree {
        return fail(&format!("{path}.suffix"), ErrorKind::Inconsistent);
    }
    if include
        .suffix
        .as_deref()
        .is_some_and(|suffix| !exact_suffix_valid(suffix))
    {
        return fail(&format!("{path}.suffix"), ErrorKind::InvalidValue);
    }
    Ok(())
}

fn validate_projection_assertion(path: &str, assertion: &ProjectionAssertion) -> Result<(), Error> {
    if !governed_name_valid(&assertion.name) {
        return fail(&format!("{path}.name"), ErrorKind::InvalidValue);
    }
    validate_projection_source(
        &format!("{path}.source"),
        assertion.projection,
        &assertion.source,
    )
}

fn validate_projection_source(
    path: &str,
    projection: ProjectionKind,
    source: &ProjectionSource,
) -> Result<(), Error> {
    match source {
        ProjectionSource::BlobLines(selection) => {
            if !safe_line_valid(selection.first_line) || !safe_line_valid(selection.last_line) {
                return fail(path, ErrorKind::InvalidValue);
            }
            if selection.first_line > selection.last_line {
                return fail(path, ErrorKind::Inconsistent);
            }
        }
        ProjectionSource::NamedRegion(selection) => {
            if !source_marker_valid(&selection.start_marker)
                || !source_marker_valid(&selection.end_marker)
            {
                return fail(path, ErrorKind::InvalidValue);
            }
            if selection.start_marker == selection.end_marker {
                return fail(path, ErrorKind::Inconsistent);
            }
        }
        ProjectionSource::TreePaths(selection) => {
            if !safe_line_valid(selection.maximum_depth)
                || !selection.suffix.as_deref().is_none_or(exact_suffix_valid)
            {
                return fail(path, ErrorKind::InvalidValue);
            }
        }
        ProjectionSource::RecordValue(selection) => {
            if !record_key_valid(&selection.key) {
                return fail(path, ErrorKind::InvalidValue);
            }
        }
        ProjectionSource::RecordSet(_) => {}
    }
    projection_source_compatible(projection, source)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::Inconsistent))
}

fn exact_suffix_valid(suffix: &str) -> bool {
    suffix.strip_prefix('.').is_some_and(|tail| {
        !tail.is_empty()
            && suffix.len() <= DOCUMENT_SUFFIX_BYTES
            && !tail.bytes().any(|byte| matches!(byte, b'/' | b'\\' | 0))
    })
}

fn safe_line_valid(line: u64) -> bool {
    (1..=json::MAX_SAFE_INTEGER.unsigned_abs()).contains(&line)
}

fn source_marker_valid(marker: &str) -> bool {
    let bytes = marker.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= SOURCE_MARKER_BYTES
        && bytes.iter().any(u8::is_ascii_graphic)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
}

fn record_key_valid(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= crate::semantic::RECORD_KEY_BYTES
        && !key.chars().any(char::is_control)
}

fn projection_source_compatible(projection: ProjectionKind, source: &ProjectionSource) -> bool {
    match source {
        ProjectionSource::BlobLines(_)
        | ProjectionSource::NamedRegion(_)
        | ProjectionSource::RecordValue(_) => projection == ProjectionKind::CodeTextV1,
        ProjectionSource::TreePaths(_) | ProjectionSource::RecordSet(_) => matches!(
            projection,
            ProjectionKind::SortedRowsV1 | ProjectionKind::DecimalCountV1
        ),
    }
}

#[must_use]
pub fn projection_source_value(source: &ProjectionSource) -> Value {
    match source {
        ProjectionSource::BlobLines(selection) => Value::Object(Box::new([
            ("kind".into(), Value::String(BLOB_LINES_SOURCE.into())),
            ("path".into(), Value::String(selection.path.as_str().into())),
            (
                "first_line".into(),
                Value::Integer(i64::try_from(selection.first_line).unwrap_or(i64::MAX)),
            ),
            (
                "last_line".into(),
                Value::Integer(i64::try_from(selection.last_line).unwrap_or(i64::MAX)),
            ),
        ])),
        ProjectionSource::NamedRegion(selection) => Value::Object(Box::new([
            ("kind".into(), Value::String(NAMED_REGION_SOURCE.into())),
            ("path".into(), Value::String(selection.path.as_str().into())),
            (
                "start_marker".into(),
                Value::String(selection.start_marker.clone().into()),
            ),
            (
                "end_marker".into(),
                Value::String(selection.end_marker.clone().into()),
            ),
        ])),
        ProjectionSource::TreePaths(selection) => {
            let mut fields = vec![
                ("kind".into(), Value::String(TREE_PATHS_SOURCE.into())),
                ("root".into(), Value::String(selection.root.as_str().into())),
                (
                    "maximum_depth".into(),
                    Value::Integer(i64::try_from(selection.maximum_depth).unwrap_or(i64::MAX)),
                ),
            ];
            if let Some(suffix) = &selection.suffix {
                fields.push(("suffix".into(), Value::String(suffix.clone().into())));
            }
            Value::Object(fields.into_boxed_slice())
        }
        ProjectionSource::RecordValue(selection) => Value::Object(Box::new([
            ("kind".into(), Value::String(RECORD_VALUE_SOURCE.into())),
            (
                "set".into(),
                Value::String(selection.set.as_str().to_owned().into()),
            ),
            ("key".into(), Value::String(selection.key.clone().into())),
        ])),
        ProjectionSource::RecordSet(selection) => Value::Object(Box::new([
            ("kind".into(), Value::String(RECORD_SET_SOURCE.into())),
            (
                "set".into(),
                Value::String(selection.set.as_str().to_owned().into()),
            ),
        ])),
    }
}
