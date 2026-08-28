use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::extraction::governed_name_valid;
use crate::json::{self, Value};
use crate::model::{Adapter, RepoPathText};

use super::{
    Disposition, IncludeKind, PromotableFindingKind, SCANNER_POLICY_SCHEMA,
    decode_disposition_rule, decode_enum, decode_items, decode_path_set, decode_repo_path, root,
    sorted_set,
};

/// Maximum UTF-8 byte length of one exact document suffix selector.
pub const DOCUMENT_SUFFIX_BYTES: usize = 64;
pub const CODE_TEXT_PROJECTION: &str = "code-text-v1";
pub const PREVIOUS_CODE_SINK: &str = "previous-code";
pub const BLOB_LINES_SOURCE: &str = "blob-lines";

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

fn decode_include(path: &str, value: Value) -> Result<DocumentInclude, Error> {
    Obj::new(path, value).and_then(|mut obj| {
        let mut include = DocumentInclude {
            path: obj.required("path", decode_repo_path)?,
            kind: obj.required("kind", decode_enum)?,
            suffix: None,
            adapter: None,
        };
        let suffix_path = obj.field("suffix");
        if let Some(value) = obj.take_optional("suffix") {
            let suffix = de::string(&suffix_path, value)?;
            if include.kind != IncludeKind::Tree {
                return fail(&suffix_path, ErrorKind::Inconsistent);
            }
            let Some(tail) = suffix.strip_prefix('.') else {
                return fail(&suffix_path, ErrorKind::InvalidValue);
            };
            if tail.is_empty()
                || suffix.len() > DOCUMENT_SUFFIX_BYTES
                || tail.bytes().any(|byte| matches!(byte, b'/' | b'\\' | 0))
            {
                return fail(&suffix_path, ErrorKind::InvalidValue);
            }
            include.suffix = Some(suffix);
        }
        include.adapter = obj
            .take_optional("adapter")
            .map(|value| decode_enum(&obj.field("adapter"), value))
            .transpose()?;
        obj.finish().map(|()| include)
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
pub struct ProjectionAssertion {
    pub document: RepoPathText,
    pub name: String,
    pub source: BlobLineSelection,
}

fn safe_line(path: &str, value: Value) -> Result<u64, Error> {
    let line = de::integer(path, value)?;
    if !(1..=json::MAX_SAFE_INTEGER).contains(&line) {
        return fail(path, ErrorKind::InvalidValue);
    }
    u64::try_from(line).map_err(|_negative| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_projection_source(path: &str, value: Value) -> Result<BlobLineSelection, Error> {
    let mut obj = Obj::new(path, value)?;
    obj.required("kind", |path, value| {
        de::const_str(path, value, BLOB_LINES_SOURCE)
    })?;
    let source = BlobLineSelection {
        path: obj.required("path", decode_repo_path)?,
        first_line: obj.required("first_line", safe_line)?,
        last_line: obj.required("last_line", safe_line)?,
    };
    obj.finish()?;
    if source.first_line <= source.last_line {
        Ok(source)
    } else {
        fail(path, ErrorKind::Inconsistent)
    }
}

fn decode_projection_assertion(path: &str, value: Value) -> Result<ProjectionAssertion, Error> {
    let mut obj = Obj::new(path, value)?;
    let document = obj.required("document", decode_repo_path)?;
    let name = obj.required("name", de::string)?;
    if !governed_name_valid(&name) {
        return fail(&obj.field("name"), ErrorKind::InvalidValue);
    }
    obj.required("projection", |path, value| {
        de::const_str(path, value, CODE_TEXT_PROJECTION)
    })?;
    obj.required("sink", |path, value| {
        de::const_str(path, value, PREVIOUS_CODE_SINK)
    })?;
    let source = obj.required("source", decode_projection_source)?;
    obj.finish()?;
    Ok(ProjectionAssertion {
        document,
        name,
        source,
    })
}

fn projection_assertion_value(assertion: ProjectionAssertion) -> Value {
    Value::Object(Box::new([
        (
            "document".into(),
            Value::String(assertion.document.as_str().into()),
        ),
        ("name".into(), Value::String(assertion.name.into())),
        (
            "projection".into(),
            Value::String(CODE_TEXT_PROJECTION.into()),
        ),
        ("sink".into(), Value::String(PREVIOUS_CODE_SINK.into())),
        (
            "source".into(),
            Value::Object(Box::new([
                ("kind".into(), Value::String(BLOB_LINES_SOURCE.into())),
                (
                    "path".into(),
                    Value::String(assertion.source.path.as_str().into()),
                ),
                (
                    "first_line".into(),
                    Value::Integer(i64::try_from(assertion.source.first_line).unwrap_or(i64::MAX)),
                ),
                (
                    "last_line".into(),
                    Value::Integer(i64::try_from(assertion.source.last_line).unwrap_or(i64::MAX)),
                ),
            ])),
        ),
    ]))
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
            .map(projection_assertion_value)
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
