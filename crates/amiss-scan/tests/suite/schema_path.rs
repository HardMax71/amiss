use std::{fs, path::Path};

/// The union path definition, exercised through the same validator the suite
/// uses, so the pair-aligned lookahead pattern is proven under this exact
/// jsonschema engine: forbidden bytes are caught at pair offsets and their
/// odd-offset lookalikes stay legal.
#[test]
fn the_schema_path_union_accepts_and_refuses_at_pair_alignment() {
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let harness = serde_json::json!({
        "$defs": schema_json["$defs"],
        "$ref": "#/$defs/RepoPath",
    });
    let validator = jsonschema::validator_for(&harness).unwrap();

    let accepted = [
        serde_json::json!("docs/guide.md"),
        serde_json::json!({"bytes_hex": "f2f2"}),
        serde_json::json!({"bytes_hex": "646f63732f62ff2e6d64"}),
        serde_json::json!({"bytes_hex": "a2f5c0"}),
    ];
    for value in accepted {
        assert!(validator.iter_errors(&value).next().is_none(), "{value}");
    }
    let refused = [
        serde_json::json!(""),
        serde_json::json!("/absolute"),
        serde_json::json!("a\\b"),
        serde_json::json!({"bytes_hex": "2f2f"}),
        serde_json::json!({"bytes_hex": "2fab"}),
        serde_json::json!({"bytes_hex": "ab2f"}),
        serde_json::json!({"bytes_hex": "ab2f2fcd"}),
        serde_json::json!({"bytes_hex": "005c"}),
        serde_json::json!({"bytes_hex": "ab00"}),
        serde_json::json!({"bytes_hex": "2e2e"}),
        serde_json::json!({"bytes_hex": "2e2e2fab"}),
        serde_json::json!({"bytes_hex": "ab2f2e2e"}),
        serde_json::json!({"bytes_hex": "F2F2"}),
        serde_json::json!({"bytes_hex": "abc"}),
        serde_json::json!({"bytes_hex": ""}),
        serde_json::json!({"bytes_hex": "f2f2", "extra": 1}),
        serde_json::json!({}),
    ];
    for value in refused {
        assert!(validator.iter_errors(&value).next().is_some(), "{value}");
    }
}
