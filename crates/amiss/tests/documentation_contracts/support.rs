#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(crate) fn report_schema() -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(repository_root().join("spec/scanner-report.schema.json"))
            .expect("report schema is readable"),
    )
    .expect("report schema is JSON")
}

pub(crate) fn schema_enum(schema: &serde_json::Value, name: &str) -> Vec<String> {
    schema
        .pointer(&format!("/$defs/{name}/enum"))
        .expect("schema enum definition exists")
        .as_array()
        .expect("schema definition is a string enum")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("schema enum member is a string")
                .to_owned()
        })
        .collect()
}
