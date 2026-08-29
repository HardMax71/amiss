use std::collections::{BTreeMap, btree_map::Entry};
use std::sync::Arc;

use serde::Deserialize as _;
use trustfall::{FieldValue, TryIntoStruct as _};
use trustfall_rustdoc_adapter::{PackageIndex, RustdocAdapter};

pub(crate) const RUSTDOC_BYTES: u64 = 33_554_432;

const PATH_QUERY: &str = r#"
{
    Crate {
        item {
            ... on Function {
                crate_id @filter(op: "=", value: ["$local_crate"])
                name @output
                signature @output
                importable_path {
                    path @output
                    public_api @filter(op: "=", value: ["$true"])
                }
            }
        }
    }
}
"#;

pub(crate) struct Normalized {
    pub complete: bool,
    pub records: BTreeMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("the Rustdoc JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("the Rustdoc format does not match the producer context")]
    Format,
    #[error("the Rustdoc target does not match the producer context")]
    Target,
    #[error("the public free-function records cannot be queried: {0}")]
    Query(String),
    #[error("a public free function has no unique canonical path/signature pair")]
    Ambiguous,
    #[error("the normalized record set exceeds the semantic evidence contract")]
    Evidence,
}

#[derive(serde::Deserialize)]
struct FunctionRow {
    name: String,
    path: Vec<String>,
    signature: String,
}

pub(crate) fn free_functions(
    bytes: &[u8],
    expected_format: u32,
    expected_target: &str,
    expected_target_triple: &str,
) -> Result<Normalized, Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let crate_ = rustdoc_types::Crate::deserialize(&mut deserializer).map_err(Error::Json)?;
    if crate_.format_version != expected_format
        || crate_.format_version != rustdoc_types::FORMAT_VERSION
    {
        return Err(Error::Format);
    }
    let root = crate_.index.get(&crate_.root).ok_or(Error::Ambiguous)?;
    if root.name.as_deref() != Some(expected_target)
        || crate_.target.triple != expected_target_triple
    {
        return Err(Error::Target);
    }
    let local_crate = u64::from(root.crate_id);

    let index = PackageIndex::from_crate(&crate_);
    let adapter = RustdocAdapter::new(&index, None);
    let rows = trustfall::execute_query(
        RustdocAdapter::schema(),
        Arc::new(&adapter),
        PATH_QUERY,
        BTreeMap::from([
            ("local_crate", FieldValue::Uint64(local_crate)),
            ("true", FieldValue::Boolean(true)),
        ]),
    )
    .map_err(|error| Error::Query(error.to_string()))?
    .map(|row| {
        row.try_into_struct::<FunctionRow>()
            .map_err(|error| Error::Query(error.to_string()))
    });
    let mut records = BTreeMap::new();
    for row in rows {
        let row = row?;
        let (key, value) = record(&row)?;
        match records.entry(key) {
            Entry::Occupied(_entry) => return Err(Error::Ambiguous),
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
        }
        if records.len() > amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT {
            return Err(Error::Evidence);
        }
    }
    Ok(Normalized {
        complete: true,
        records,
    })
}

fn record(row: &FunctionRow) -> Result<(String, String), Error> {
    if row.path.is_empty()
        || row
            .path
            .iter()
            .any(|component| component.is_empty() || component.chars().any(char::is_control))
    {
        return Err(Error::Ambiguous);
    }
    let path = row.path.join("::");
    let mut signature = String::with_capacity(row.signature.len());
    for word in row.signature.split_ascii_whitespace() {
        if !signature.is_empty() {
            signature.push(' ');
        }
        signature.push_str(word);
    }
    let declaration = format!("fn {}", row.name);
    let Some((qualifiers, suffix)) = signature.split_once(&declaration) else {
        return Err(Error::Ambiguous);
    };
    if qualifiers.contains("fn ") || signature.starts_with("pub ") {
        return Err(Error::Ambiguous);
    }
    let key = format!("fn/{path}");
    let value = format!("pub {qualifiers}fn {path}{suffix}");
    if key.len() > amiss_wire::semantic::RECORD_KEY_BYTES
        || value.len() > amiss_wire::semantic::RECORD_VALUE_BYTES
        || key.chars().any(char::is_control)
        || value.chars().any(char::is_control)
    {
        return Err(Error::Evidence);
    }
    Ok((key, value))
}

mod tests;
