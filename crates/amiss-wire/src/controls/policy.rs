use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::extraction::governed_name_valid;
use crate::json::{self, Value};
use crate::model::{Adapter, ArtifactId, RepoPathText};

use super::{
    Disposition, IncludeKind, PromotableFindingKind, SCANNER_POLICY_SCHEMA,
    decode_disposition_rule, decode_enum, decode_items, decode_path_set, decode_repo_path, root,
    sorted_set,
};

/// Maximum UTF-8 byte length of one exact document suffix selector.
pub const DOCUMENT_SUFFIX_BYTES: usize = 64;
pub const PREVIOUS_CODE_SINK: &str = "previous-code";
pub const BLOB_LINES_SOURCE: &str = "blob-lines";
pub const NAMED_REGION_SOURCE: &str = "named-region";
pub const TREE_PATHS_SOURCE: &str = "tree-paths";
pub const RECORD_VALUE_SOURCE: &str = "record-value";
pub const SOURCE_MARKER_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr, strum::EnumString)]
pub enum ProjectionKind {
    #[strum(serialize = "code-text-v1")]
    CodeTextV1,
    #[strum(serialize = "sorted-rows-v1")]
    SortedRowsV1,
    #[strum(serialize = "decimal-count-v1")]
    DecimalCountV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInclude {
    pub path: RepoPathText,
    pub kind: IncludeKind,
    pub suffix: Option<String>,
    pub adapter: Option<Adapter>,
}

/// Projects one validated include row through the scanner-policy wire shape.
#[must_use]
pub fn document_include_value(include: DocumentInclude) -> Value {
    let mut fields = vec![
        ("path".into(), Value::String(include.path.as_str().into())),
        ("kind".into(), Value::String(include.kind.as_ref().into())),
    ];
    if let Some(suffix) = include.suffix {
        fields.push(("suffix".into(), Value::String(suffix.into())));
    }
    if let Some(adapter) = include.adapter {
        fields.push(("adapter".into(), Value::String(adapter.as_ref().into())));
    }
    Value::Object(fields.into_boxed_slice())
}

fn exact_suffix(path: &str, value: Value) -> Result<String, Error> {
    let suffix = de::string(path, value)?;
    let Some(tail) = suffix.strip_prefix('.') else {
        return fail(path, ErrorKind::InvalidValue);
    };
    if tail.is_empty()
        || suffix.len() > DOCUMENT_SUFFIX_BYTES
        || tail.bytes().any(|byte| matches!(byte, b'/' | b'\\' | 0))
    {
        return fail(path, ErrorKind::InvalidValue);
    }
    Ok(suffix)
}

fn decode_include(path: &str, value: Value) -> Result<DocumentInclude, Error> {
    let mut obj = Obj::new(path, value)?;
    let include_path = obj.required("path", decode_repo_path)?;
    let kind = obj.required("kind", decode_enum)?;
    let suffix_path = obj.field("suffix");
    let raw_suffix = obj.take_optional("suffix");
    if raw_suffix.is_some() && kind != IncludeKind::Tree {
        return fail(&suffix_path, ErrorKind::Inconsistent);
    }
    let suffix = raw_suffix
        .map(|value| exact_suffix(&suffix_path, value))
        .transpose()?;
    let adapter = obj
        .take_optional("adapter")
        .map(|value| decode_enum(&obj.field("adapter"), value))
        .transpose()?;
    obj.finish()?;
    Ok(DocumentInclude {
        path: include_path,
        kind,
        suffix,
        adapter,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingDisposition {
    pub finding_kind: PromotableFindingKind,
    pub disposition: Disposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobLineSelection {
    pub path: RepoPathText,
    pub first_line: u64,
    pub last_line: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedRegionSelection {
    pub path: RepoPathText,
    pub start_marker: String,
    pub end_marker: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreePathSelection {
    pub root: RepoPathText,
    pub suffix: Option<String>,
    pub maximum_depth: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordValueSelection {
    pub set: ArtifactId,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionSource {
    BlobLines(BlobLineSelection),
    NamedRegion(NamedRegionSelection),
    TreePaths(TreePathSelection),
    RecordValue(RecordValueSelection),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionAssertion {
    pub document: RepoPathText,
    pub name: String,
    pub projection: ProjectionKind,
    pub source: ProjectionSource,
}

fn safe_line(path: &str, value: Value) -> Result<u64, Error> {
    let line = de::integer(path, value)?;
    if !(1..=json::MAX_SAFE_INTEGER).contains(&line) {
        return fail(path, ErrorKind::InvalidValue);
    }
    u64::try_from(line).map_err(|_negative| Error::new(path, ErrorKind::InvalidValue))
}

fn source_marker(path: &str, value: Value) -> Result<String, Error> {
    let marker = de::string(path, value)?;
    let bytes = marker.as_bytes();
    if bytes.is_empty()
        || bytes.len() > SOURCE_MARKER_BYTES
        || !bytes.iter().any(u8::is_ascii_graphic)
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        return fail(path, ErrorKind::InvalidValue);
    }
    Ok(marker)
}

fn decode_projection_source(path: &str, value: Value) -> Result<ProjectionSource, Error> {
    let mut obj = Obj::new(path, value)?;
    let kind_path = obj.field("kind");
    let kind = obj.required("kind", de::string)?;
    let source = if kind == BLOB_LINES_SOURCE {
        let selection = BlobLineSelection {
            path: obj.required("path", decode_repo_path)?,
            first_line: obj.required("first_line", safe_line)?,
            last_line: obj.required("last_line", safe_line)?,
        };
        if selection.first_line > selection.last_line {
            return fail(path, ErrorKind::Inconsistent);
        }
        ProjectionSource::BlobLines(selection)
    } else if kind == NAMED_REGION_SOURCE {
        let selection = NamedRegionSelection {
            path: obj.required("path", decode_repo_path)?,
            start_marker: obj.required("start_marker", source_marker)?,
            end_marker: obj.required("end_marker", source_marker)?,
        };
        if selection.start_marker == selection.end_marker {
            return fail(path, ErrorKind::Inconsistent);
        }
        ProjectionSource::NamedRegion(selection)
    } else if kind == TREE_PATHS_SOURCE {
        let suffix_path = obj.field("suffix");
        ProjectionSource::TreePaths(TreePathSelection {
            root: obj.required("root", decode_repo_path)?,
            suffix: obj
                .take_optional("suffix")
                .map(|value| exact_suffix(&suffix_path, value))
                .transpose()?,
            maximum_depth: obj.required("maximum_depth", safe_line)?,
        })
    } else if kind == RECORD_VALUE_SOURCE {
        ProjectionSource::RecordValue(RecordValueSelection {
            set: obj.required("set", |path, value| {
                ArtifactId::new(de::string(path, value)?)
                    .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
            })?,
            key: obj.required("key", |path, value| {
                let key = de::string(path, value)?;
                if key.is_empty()
                    || key.len() > crate::semantic::RECORD_KEY_BYTES
                    || key.chars().any(char::is_control)
                {
                    return fail(path, ErrorKind::InvalidValue);
                }
                Ok(key)
            })?,
        })
    } else {
        return fail(&kind_path, ErrorKind::InvalidValue);
    };
    obj.finish()?;
    Ok(source)
}

fn decode_projection_assertion(path: &str, value: Value) -> Result<ProjectionAssertion, Error> {
    let mut obj = Obj::new(path, value)?;
    let document = obj.required("document", decode_repo_path)?;
    let name = obj.required("name", de::string)?;
    if !governed_name_valid(&name) {
        return fail(&obj.field("name"), ErrorKind::InvalidValue);
    }
    let projection = obj.required("projection", decode_enum)?;
    obj.required("sink", |path, value| {
        de::const_str(path, value, PREVIOUS_CODE_SINK)
    })?;
    let source = obj.required("source", decode_projection_source)?;
    let compatible = match &source {
        ProjectionSource::BlobLines(_)
        | ProjectionSource::NamedRegion(_)
        | ProjectionSource::RecordValue(_) => projection == ProjectionKind::CodeTextV1,
        ProjectionSource::TreePaths(_) => matches!(
            projection,
            ProjectionKind::SortedRowsV1 | ProjectionKind::DecimalCountV1
        ),
    };
    if !compatible {
        return fail(path, ErrorKind::Inconsistent);
    }
    obj.finish()?;
    Ok(ProjectionAssertion {
        document,
        name,
        projection,
        source,
    })
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
    }
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
        let include_rows: Vec<Value> = document_includes
            .into_iter()
            .map(document_include_value)
            .collect();
        let inventory: Vec<Value> = protected_inventory
            .into_iter()
            .map(|path| Value::String(path.as_str().into()))
            .collect();
        let assertions: Vec<Value> = projection_assertions
            .into_iter()
            .map(|assertion| {
                Value::Object(Box::new([
                    (
                        "document".into(),
                        Value::String(assertion.document.as_str().into()),
                    ),
                    ("name".into(), Value::String(assertion.name.into())),
                    (
                        "projection".into(),
                        Value::String(assertion.projection.as_ref().into()),
                    ),
                    ("sink".into(), Value::String(PREVIOUS_CODE_SINK.into())),
                    ("source".into(), projection_source_value(&assertion.source)),
                ]))
            })
            .collect();
        let dispositions: Vec<Value> = finding_dispositions
            .into_iter()
            .map(|row| {
                Value::Object(Box::new([
                    (
                        "finding_kind".into(),
                        Value::String(row.finding_kind.as_ref().into()),
                    ),
                    (
                        "disposition".into(),
                        Value::String(row.disposition.as_ref().into()),
                    ),
                ]))
            })
            .collect();
        let value = Value::Object(Box::new([
            ("schema".into(), Value::String(SCANNER_POLICY_SCHEMA.into())),
            (
                "document_includes".into(),
                Value::Array(include_rows.into_boxed_slice()),
            ),
            (
                "projection_assertions".into(),
                Value::Array(assertions.into_boxed_slice()),
            ),
            (
                "protected_inventory".into(),
                Value::Array(inventory.into_boxed_slice()),
            ),
            (
                "finding_dispositions".into(),
                Value::Array(dispositions.into_boxed_slice()),
            ),
        ]));
        Self::parse(&json::canonical(&value))
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
        let value = root(bytes)?;
        let digest = hj(SCANNER_POLICY_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, SCANNER_POLICY_SCHEMA)
        })?;

        let includes_path = obj.field("document_includes");
        let includes = de::array(&includes_path, obj.take("document_includes")?)?;
        let document_includes = decode_items(&includes_path, includes, 100_000, decode_include)?;
        sorted_set(&includes_path, &document_includes, |a, b| {
            (a.path.as_str(), a.kind).cmp(&(b.path.as_str(), b.kind))
        })?;

        let assertions_path = obj.field("projection_assertions");
        let projection_assertions = match obj.take_optional("projection_assertions") {
            Some(value) => decode_items(
                &assertions_path,
                de::array(&assertions_path, value)?,
                100_000,
                decode_projection_assertion,
            )?,
            None => Vec::new(),
        };
        sorted_set(&assertions_path, &projection_assertions, |left, right| {
            (left.document.as_str(), left.name.as_str())
                .cmp(&(right.document.as_str(), right.name.as_str()))
        })?;

        let inventory_path = obj.field("protected_inventory");
        let protected_inventory =
            decode_path_set(&inventory_path, obj.take("protected_inventory")?)?;

        let dispositions_path = obj.field("finding_dispositions");
        let raw = de::array(&dispositions_path, obj.take("finding_dispositions")?)?;
        let finding_dispositions =
            decode_items(&dispositions_path, raw, 3, decode_disposition_rule)?;
        sorted_set(&dispositions_path, &finding_dispositions, |a, b| {
            a.finding_kind.as_ref().cmp(b.finding_kind.as_ref())
        })?;

        obj.finish()?;
        Ok(Self {
            digest,
            document_includes,
            projection_assertions,
            protected_inventory,
            finding_dispositions,
        })
    }
}
