#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(crate) fn command_grammar() -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .arg("--help")
        .output()
        .expect("the command prints its grammar");
    assert!(output.status.success(), "the help query succeeds");
    assert!(output.stderr.is_empty(), "help writes no diagnostic");
    String::from_utf8(output.stdout)
        .expect("the grammar is UTF-8")
        .strip_suffix('\n')
        .expect("the grammar ends with one newline")
        .to_owned()
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
