#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned schema fragments"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static REPORT_SCHEMA: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::from_slice(
        &fs::read(repository_root().join("spec/scanner-report.schema.json"))
            .expect("the report schema is readable"),
    )
    .expect("the report schema is JSON")
});

static REPORT_VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    jsonschema::validator_for(&REPORT_SCHEMA).expect("the report schema compiles")
});

pub(crate) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(crate) fn fixture_bytes(name: &str) -> Vec<u8> {
    fs::read(repository_root().join("spec/examples").join(name))
        .expect("the specification ships this fixture")
}

#[track_caller]
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

pub(crate) fn generated_report(
    bytes: &[u8],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    amiss_wire::report::validate_envelope(bytes)?;
    let value = serde_json::from_slice(bytes)?;
    REPORT_VALIDATOR
        .validate(&value)
        .map_err(|error| format!("{}: {error}", error.instance_path()))?;
    let canonical = serde_json_canonicalizer::to_vec(&value)?;
    if bytes.strip_suffix(b"\n") != Some(canonical.as_slice()) {
        return Err("the generated report must be canonical JSON followed by one newline".into());
    }
    Ok(value)
}

pub(crate) struct ReportSchemaFragment {
    definition: String,
    validator: jsonschema::Validator,
}

impl ReportSchemaFragment {
    pub(crate) fn new(definition: &str) -> Self {
        let schema = &*REPORT_SCHEMA;
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
