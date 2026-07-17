#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned schema fragments"
)]
#![expect(
    dead_code,
    reason = "each integration-test crate uses a different subset of this shared support module"
)]

use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(crate) fn fixture_bytes(name: &str) -> Vec<u8> {
    fs::read(repository_root().join("spec/examples").join(name))
        .expect("the specification ships this fixture")
}

fn report_schema() -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(repository_root().join("spec/scanner-report.schema.json"))
            .expect("the report schema is readable"),
    )
    .expect("the report schema is JSON")
}

fn assert_valid(validator: &jsonschema::Validator, value: &serde_json::Value, label: &str) {
    let defects: Vec<String> = validator
        .iter_errors(value)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert!(
        defects.is_empty(),
        "{label} violates its report schema:\n{}",
        defects.join("\n"),
    );
}

pub(crate) fn assert_report(value: &serde_json::Value, label: &str) {
    let validator =
        jsonschema::validator_for(&report_schema()).expect("the report schema compiles");
    assert_valid(&validator, value, label);
}

pub(crate) struct ReportSchemaFragment {
    definition: String,
    validator: jsonschema::Validator,
}

impl ReportSchemaFragment {
    pub(crate) fn new(definition: &str) -> Self {
        let schema = report_schema();
        let harness = serde_json::json!({
            "$schema": schema
                .get("$schema")
                .expect("the report schema declares its dialect"),
            "$defs": schema
                .get("$defs")
                .expect("the report schema publishes fragment definitions"),
            "$ref": format!("#/$defs/{definition}"),
        });
        Self {
            definition: definition.to_owned(),
            validator: jsonschema::validator_for(&harness)
                .expect("the report-schema fragment compiles"),
        }
    }

    pub(crate) fn assert_value(&self, value: &serde_json::Value, label: &str) {
        assert_valid(
            &self.validator,
            value,
            &format!("{label} against $defs/{}", self.definition),
        );
    }
}
