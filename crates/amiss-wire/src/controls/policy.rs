use super::mapping::wire_fields;
use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::codec::{self, MAX_SAFE_INTEGER, rule};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hj};
use crate::extraction::governed_name_valid;
use crate::json::Value;
use crate::model::{Adapter, ArtifactId, RepoPathText};
use crate::semantic::RECORD_KEY_BYTES;

use super::{
    Disposition, IncludeKind, PromotableFindingKind, SCANNER_POLICY_SCHEMA, check_len,
    check_schema, non_null, root, sorted_set,
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

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::AsRefStr,
    strum::EnumIter,
    strum::EnumString,
    Serialize,
    Deserialize,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentInclude {
    pub path: RepoPathText,
    pub kind: IncludeKind,
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub suffix: Option<String>,
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub adapter: Option<Adapter>,
}

/// Projects one validated include row through the scanner-policy wire shape.
///
/// # Errors
///
/// The include cannot be represented in the strict JSON profile.
pub fn document_include_value(include: &DocumentInclude) -> Result<Value, Error> {
    codec::to_value(include)
}

fn valid_suffix<C>(value: &str, _context: &C) -> garde::Result {
    let named = value.strip_prefix('.').is_some_and(|tail| {
        !tail.is_empty() && !tail.bytes().any(|byte| matches!(byte, b'/' | b'\\' | 0))
    });
    rule(named, "suffix must be one dot-led extension")
}

fn valid_marker<C>(value: &str, _context: &C) -> garde::Result {
    let bytes = value.as_bytes();
    let visible = bytes.iter().any(u8::is_ascii_graphic)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_graphic() || *byte == b' ');
    rule(visible, "marker must be visible ASCII")
}

fn valid_record_key<C>(value: &str, _context: &C) -> garde::Result {
    rule(
        !value.chars().any(char::is_control),
        "record key holds a control character",
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingDisposition {
    pub finding_kind: PromotableFindingKind,
    pub disposition: Disposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
#[serde(remote = "Self")]
pub struct BlobLineSelection {
    pub path: RepoPathText,
    #[garde(range(min = 1, max = MAX_SAFE_INTEGER))]
    pub first_line: u64,
    #[garde(range(min = 1, max = MAX_SAFE_INTEGER))]
    pub last_line: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
#[serde(remote = "Self")]
pub struct NamedRegionSelection {
    pub path: RepoPathText,
    #[garde(length(bytes, min = 1, max = SOURCE_MARKER_BYTES), custom(valid_marker))]
    pub start_marker: String,
    #[garde(length(bytes, min = 1, max = SOURCE_MARKER_BYTES), custom(valid_marker))]
    pub end_marker: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
#[serde(remote = "Self")]
pub struct TreePathSelection {
    pub root: RepoPathText,
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    #[garde(inner(length(bytes, max = DOCUMENT_SUFFIX_BYTES), custom(valid_suffix)))]
    pub suffix: Option<String>,
    #[garde(range(min = 1, max = MAX_SAFE_INTEGER))]
    pub maximum_depth: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
#[serde(remote = "Self")]
pub struct RecordValueSelection {
    pub set: ArtifactId,
    #[garde(length(bytes, min = 1, max = RECORD_KEY_BYTES), custom(valid_record_key))]
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
#[serde(remote = "Self")]
pub struct RecordSetSelection {
    pub set: ArtifactId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[garde(allow_unvalidated)]
#[serde(remote = "Self")]
pub enum ProjectionSource {
    BlobLines(#[garde(dive)] BlobLineSelection),
    NamedRegion(#[garde(dive)] NamedRegionSelection),
    TreePaths(#[garde(dive)] TreePathSelection),
    RecordValue(#[garde(dive)] RecordValueSelection),
    RecordSet(#[garde(dive)] RecordSetSelection),
}

impl ProjectionSource {
    /// # Errors
    ///
    /// A blob range runs backwards or both region markers are one string.
    pub fn check(&self, path: &str) -> Result<(), Error> {
        let consistent = match self {
            Self::BlobLines(selection) => selection.first_line <= selection.last_line,
            Self::NamedRegion(selection) => selection.start_marker != selection.end_marker,
            Self::TreePaths(_) | Self::RecordValue(_) | Self::RecordSet(_) => true,
        };
        if consistent {
            Ok(())
        } else {
            fail(path, ErrorKind::Inconsistent)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Assertion", into = "Assertion")]
pub struct ProjectionAssertion {
    pub document: RepoPathText,
    pub name: String,
    pub projection: ProjectionKind,
    pub source: ProjectionSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Assertion {
    document: RepoPathText,
    name: String,
    projection: ProjectionKind,
    sink: AssertionSink,
    source: ProjectionSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum AssertionSink {
    #[serde(rename = "previous-code")]
    PreviousCode,
}

wire_fields! {
    Assertion <=> ProjectionAssertion (input) {
        fields [document, name, projection, source],
        mapped [],
        wire { sink: AssertionSink::PreviousCode },
        domain {}
    }
}

pub(crate) fn compatible_source(
    projection: ProjectionKind,
    source: &ProjectionSource,
    path: &str,
) -> Result<(), Error> {
    let compatible = match source {
        ProjectionSource::BlobLines(_)
        | ProjectionSource::NamedRegion(_)
        | ProjectionSource::RecordValue(_) => projection == ProjectionKind::CodeTextV1,
        ProjectionSource::TreePaths(_) | ProjectionSource::RecordSet(_) => matches!(
            projection,
            ProjectionKind::SortedRowsV1 | ProjectionKind::DecimalCountV1
        ),
    };
    if compatible {
        Ok(())
    } else {
        fail(path, ErrorKind::Inconsistent)
    }
}

/// Checks a directly constructed source through the same closed grammar and
/// projection compatibility laws as a scanner-policy assertion.
///
/// # Errors
///
/// The source violates its field constraints or laws, or is incompatible
/// with the selected projection.
pub fn check_projection_source(
    projection: ProjectionKind,
    source: &ProjectionSource,
) -> Result<(), Error> {
    codec::constrained(source, "$")?;
    source.check("$")?;
    compatible_source(projection, source, "$")
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
    let source: ProjectionSource = codec::decode(bytes)?;
    check_projection_source(projection, &source)?;
    Ok(source)
}

/// Projects one source through its typed wire shape.
///
/// # Errors
///
/// The source cannot be represented in the strict JSON profile.
pub fn projection_source_value(source: &ProjectionSource) -> Result<Value, Error> {
    codec::to_value(source)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannerPolicy {
    digest: Digest,
    document_includes: Vec<DocumentInclude>,
    projection_assertions: Vec<ProjectionAssertion>,
    protected_inventory: Vec<RepoPathText>,
    finding_dispositions: Vec<FindingDisposition>,
}

impl ScannerPolicy {
    /// Builds a policy through the same ordering, uniqueness, and digest laws
    /// used for repository-controlled bytes.
    ///
    /// # Errors
    ///
    /// The supplied sets contain duplicates or otherwise fail the
    /// scanner-policy grammar.
    pub fn new(
        mut document_includes: Vec<DocumentInclude>,
        mut projection_assertions: Vec<ProjectionAssertion>,
        mut protected_inventory: Vec<RepoPathText>,
        mut finding_dispositions: Vec<FindingDisposition>,
    ) -> Result<Self, Error> {
        document_includes.sort_by(|left, right| {
            (left.path.as_str(), left.kind).cmp(&(right.path.as_str(), right.kind))
        });
        projection_assertions.sort_by(|left, right| {
            (left.document.as_str(), left.name.as_str())
                .cmp(&(right.document.as_str(), right.name.as_str()))
        });
        protected_inventory.sort();
        finding_dispositions
            .sort_by(|left, right| left.finding_kind.as_ref().cmp(right.finding_kind.as_ref()));
        let payload = Policy {
            schema: SCANNER_POLICY_SCHEMA.to_owned(),
            document_includes,
            projection_assertions: Some(projection_assertions),
            protected_inventory,
            finding_dispositions,
        };
        payload.check()?;
        let digest = codec::digest(SCANNER_POLICY_SCHEMA, &payload)?;
        Ok(Self::from_payload(payload, digest))
    }

    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    #[must_use]
    pub fn document_includes(&self) -> &[DocumentInclude] {
        &self.document_includes
    }

    #[must_use]
    pub fn protected_inventory(&self) -> &[RepoPathText] {
        &self.protected_inventory
    }

    #[must_use]
    pub fn projection_assertions(&self) -> &[ProjectionAssertion] {
        &self.projection_assertions
    }

    #[must_use]
    pub fn finding_dispositions(&self) -> &[FindingDisposition] {
        &self.finding_dispositions
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, unknown fields,
    /// invalid grammar values, and unsorted or duplicate set members.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::from_value(&root(bytes)?)
    }

    /// Checks an already decoded scanner policy without serializing it again.
    ///
    /// # Errors
    ///
    /// The policy has an invalid shape, value, ordering, or duplicate member.
    pub fn from_value(value: &Value) -> Result<Self, Error> {
        let payload: Policy = codec::from_value("$", value)?;
        payload.check()?;
        Ok(Self::from_payload(
            payload,
            hj(SCANNER_POLICY_SCHEMA, value),
        ))
    }

    fn from_payload(payload: Policy, digest: Digest) -> Self {
        Self {
            digest,
            document_includes: payload.document_includes,
            projection_assertions: payload.projection_assertions.unwrap_or_default(),
            protected_inventory: payload.protected_inventory,
            finding_dispositions: payload.finding_dispositions,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    document_includes: Vec<DocumentInclude>,
    #[serde(
        default,
        deserialize_with = "non_null",
        skip_serializing_if = "Option::is_none"
    )]
    projection_assertions: Option<Vec<ProjectionAssertion>>,
    protected_inventory: Vec<RepoPathText>,
    finding_dispositions: Vec<FindingDisposition>,
}

impl Policy {
    fn check(&self) -> Result<(), Error> {
        check_schema("$.schema", &self.schema, SCANNER_POLICY_SCHEMA)?;
        check_len("$.document_includes", self.document_includes.len(), 100_000)?;
        for (index, include) in self.document_includes.iter().enumerate() {
            if let Some(suffix) = &include.suffix {
                let path = format!("$.document_includes[{index}].suffix");
                if include.kind != IncludeKind::Tree {
                    return fail(&path, ErrorKind::Inconsistent);
                }
                if suffix.len() > DOCUMENT_SUFFIX_BYTES || valid_suffix(suffix, &()).is_err() {
                    return fail(&path, ErrorKind::InvalidValue);
                }
            }
        }
        sorted_set("$.document_includes", &self.document_includes, |a, b| {
            (a.path.as_str(), a.kind).cmp(&(b.path.as_str(), b.kind))
        })?;
        let assertions = self.projection_assertions.as_deref().unwrap_or_default();
        check_len("$.projection_assertions", assertions.len(), 100_000)?;
        for (index, assertion) in assertions.iter().enumerate() {
            let path = format!("$.projection_assertions[{index}]");
            if !governed_name_valid(&assertion.name) {
                return fail(&format!("{path}.name"), ErrorKind::InvalidValue);
            }
            let source_path = format!("{path}.source");
            codec::constrained(&assertion.source, &source_path)?;
            assertion.source.check(&source_path)?;
            compatible_source(assertion.projection, &assertion.source, &path)?;
        }
        sorted_set("$.projection_assertions", assertions, |a, b| {
            (a.document.as_str(), a.name.as_str()).cmp(&(b.document.as_str(), b.name.as_str()))
        })?;
        check_len(
            "$.protected_inventory",
            self.protected_inventory.len(),
            100_000,
        )?;
        sorted_set("$.protected_inventory", &self.protected_inventory, Ord::cmp)?;
        check_len("$.finding_dispositions", self.finding_dispositions.len(), 3)?;
        sorted_set(
            "$.finding_dispositions",
            &self.finding_dispositions,
            |a, b| a.finding_kind.as_ref().cmp(b.finding_kind.as_ref()),
        )
    }
}

impl Serialize for ProjectionSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ProjectionSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

impl Serialize for BlobLineSelection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for BlobLineSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

impl Serialize for NamedRegionSelection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for NamedRegionSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

impl Serialize for TreePathSelection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for TreePathSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

impl Serialize for RecordValueSelection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RecordValueSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}

impl Serialize for RecordSetSelection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RecordSetSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(codec::object(deserializer))
    }
}
