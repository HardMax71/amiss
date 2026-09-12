use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::extraction::governed_name_valid;

use crate::model::{Adapter, ArtifactId, RepoPathText};

use super::{Disposition, IncludeKind, PromotableFindingKind, sorted_set};

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
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ScannerPolicySchema {
    #[strum(serialize = "amiss/scanner-policy")]
    Current,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    strum::AsRefStr,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ProjectionKind {
    #[strum(serialize = "code-text-v1")]
    CodeTextV1,
    #[strum(serialize = "sorted-rows-v1")]
    SortedRowsV1,
    #[strum(serialize = "decimal-count-v1")]
    DecimalCountV1,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ProjectionSink {
    #[strum(serialize = "previous-code")]
    PreviousCode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct DocumentInclude {
    pub path: RepoPathText,
    pub kind: IncludeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<Adapter>,
}

impl Serialize for DocumentInclude {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for DocumentInclude {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct FindingDisposition {
    pub finding_kind: PromotableFindingKind,
    pub disposition: Disposition,
}

impl Serialize for FindingDisposition {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for FindingDisposition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct BlobLineSelection {
    pub path: RepoPathText,
    pub first_line: u64,
    pub last_line: u64,
}

impl Serialize for BlobLineSelection {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for BlobLineSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct NamedRegionSelection {
    pub path: RepoPathText,
    pub start_marker: String,
    pub end_marker: String,
}

impl Serialize for NamedRegionSelection {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for NamedRegionSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct TreePathSelection {
    pub root: RepoPathText,
    #[serde(
        default,
        deserialize_with = "json_serde::deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub suffix: Option<String>,
    pub maximum_depth: u64,
}

impl Serialize for TreePathSelection {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for TreePathSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct RecordValueSelection {
    pub set: ArtifactId,
    pub key: String,
}

impl Serialize for RecordValueSelection {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RecordValueSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct RecordSetSelection {
    pub set: ArtifactId,
}

impl Serialize for RecordSetSelection {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for RecordSetSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    remote = "Self",
    tag = "kind",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum ProjectionSource {
    BlobLines(BlobLineSelection),
    NamedRegion(NamedRegionSelection),
    TreePaths(TreePathSelection),
    RecordValue(RecordValueSelection),
    RecordSet(RecordSetSelection),
}

impl Serialize for ProjectionSource {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ProjectionSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct ProjectionAssertion {
    pub document: RepoPathText,
    pub name: String,
    pub projection: ProjectionKind,
    pub sink: ProjectionSink,
    pub source: ProjectionSource,
}

impl Serialize for ProjectionAssertion {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ProjectionAssertion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
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

impl Serialize for ScannerPolicy {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for ScannerPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// Parses and validates one repository scanner policy.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, unknown fields,
/// invalid grammar values, and unsorted or duplicate set members.
pub fn parse_scanner_policy(bytes: &[u8]) -> Result<ScannerPolicy, Error> {
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let policy: ScannerPolicy = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
    policy.validate()?;
    Ok(policy)
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

impl ScannerPolicy {
    /// Checks this control's domain rules and resource limits.
    ///
    /// # Errors
    ///
    /// A public field violates the contract enforced by [`parse_scanner_policy`].
    pub fn validate(&self) -> Result<(), Error> {
        if self.document_includes.len() > 100_000 {
            return fail("$.document_includes", ErrorKind::LimitExceeded);
        }
        for (index, include) in self.document_includes.iter().enumerate() {
            validate_document_include(&format!("$.document_includes[{index}]"), include)?;
        }
        sorted_set(
            "$.document_includes",
            &self.document_includes,
            |left, right| (left.path.as_str(), left.kind).cmp(&(right.path.as_str(), right.kind)),
        )?;

        let assertions = self.projection_assertions.as_deref().unwrap_or_default();
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

        if self.protected_inventory.len() > 100_000 {
            return fail("$.protected_inventory", ErrorKind::LimitExceeded);
        }
        sorted_set(
            "$.protected_inventory",
            &self.protected_inventory,
            |left, right| left.as_str().cmp(right.as_str()),
        )?;

        if self.finding_dispositions.len() > 3 {
            return fail("$.finding_dispositions", ErrorKind::LimitExceeded);
        }
        sorted_set(
            "$.finding_dispositions",
            &self.finding_dispositions,
            |left, right| left.finding_kind.as_ref().cmp(right.finding_kind.as_ref()),
        )
    }
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
    (1..=js_int::MAX_SAFE_INT.unsigned_abs()).contains(&line)
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
