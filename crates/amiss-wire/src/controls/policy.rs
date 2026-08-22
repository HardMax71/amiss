use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{self, Value};
use crate::model::{Adapter, RepoPathText};

use super::{
    Disposition, IncludeKind, PromotableFindingKind, SCANNER_POLICY_SCHEMA,
    decode_disposition_rule, decode_enum, decode_items, decode_path_set, decode_repo_path, root,
    sorted_set,
};

/// Maximum UTF-8 byte length of one exact document suffix selector.
pub const DOCUMENT_SUFFIX_BYTES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInclude {
    pub path: RepoPathText,
    pub kind: IncludeKind,
    pub suffix: Option<String>,
    pub adapter: Option<Adapter>,
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
pub struct ScannerPolicy {
    digest: Digest,
    document_includes: Vec<DocumentInclude>,
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
        mut protected_inventory: Vec<RepoPathText>,
        mut finding_dispositions: Vec<FindingDisposition>,
    ) -> Result<Self, Error> {
        document_includes.sort_by(|left, right| {
            (left.path.as_str(), left.kind).cmp(&(right.path.as_str(), right.kind))
        });
        protected_inventory.sort();
        finding_dispositions
            .sort_by(|left, right| left.finding_kind.as_ref().cmp(right.finding_kind.as_ref()));
        let include_rows: Vec<Value> = document_includes
            .into_iter()
            .map(|include| {
                let mut rows = vec![
                    ("path".into(), Value::String(include.path.as_str().into())),
                    ("kind".into(), Value::String(include.kind.as_ref().into())),
                ];
                if let Some(suffix) = include.suffix {
                    rows.push(("suffix".into(), Value::String(suffix.into())));
                }
                if let Some(adapter) = include.adapter {
                    rows.push(("adapter".into(), Value::String(adapter.as_ref().into())));
                }
                Value::Object(rows.into_boxed_slice())
            })
            .collect();
        let inventory: Vec<Value> = protected_inventory
            .into_iter()
            .map(|path| Value::String(path.as_str().into()))
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
            protected_inventory,
            finding_dispositions,
        })
    }
}
