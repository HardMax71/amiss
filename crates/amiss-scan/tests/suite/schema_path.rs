use std::{fs, path::Path};

/// The union path definition, exercised through the same validator the suite
/// uses: text, or the raw bytes as an array of byte values. The byte grammar
/// is the writer's law, since an array admits no pattern.
#[test]
fn the_schema_path_union_accepts_text_or_a_byte_array() {
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
        serde_json::json!({"bytes": [242, 242]}),
        serde_json::json!({"bytes": b"docs/b\xff.md"}),
        serde_json::json!({"bytes": vec![255_u8; 4096]}),
    ];
    for value in accepted {
        assert!(validator.iter_errors(&value).next().is_none(), "{value}");
    }
    let refused = [
        serde_json::json!(""),
        serde_json::json!("/absolute"),
        serde_json::json!("a\\b"),
        serde_json::json!({"bytes": []}),
        serde_json::json!({"bytes": [256]}),
        serde_json::json!({"bytes": [-1]}),
        serde_json::json!({"bytes": [1.5]}),
        serde_json::json!({"bytes": "f2f2"}),
        serde_json::json!({"bytes": vec![255_u8; 4097]}),
        serde_json::json!({"bytes_hex": "f2f2"}),
        serde_json::json!({"bytes": [242], "extra": 1}),
        serde_json::json!({}),
    ];
    for value in refused {
        assert!(validator.iter_errors(&value).next().is_some(), "{value}");
    }
}
