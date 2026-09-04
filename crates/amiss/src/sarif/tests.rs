#![cfg(test)]

use amiss_wire::json::parse;

#[test]
fn typed_sarif_preserves_optional_fields_and_canonical_order() {
    let envelope = parse(
        br#"{
        "payload": {
            "result": {"complete": false, "exit_code": 2},
            "errors": [{"code": "READ_FAILED", "description": "could not read"}],
            "findings": [
                {
                    "kind": "explicit-target-missing", "finding_key": "sha256:a",
                    "description": "missing \"target\"\n", "effective_disposition": "fail",
                    "location": {"path": "docs/a b.md", "span": {
                        "start_line": 2, "start_column": 3, "end_line": 2, "end_column": 8
                    }},
                    "fix": {"path": "docs/a b.md", "description": "repair", "replacement": "",
                        "span": {"start_byte": 4, "end_byte": 7}}
                },
                {
                    "kind": "future-kind", "finding_key": "sha256:b",
                    "description": "future", "effective_disposition": "warn",
                    "location": {"path": {"bytes_hex": "ff2e6d64"}, "span": null},
                    "fix": {"path": {"bytes_hex": "ff2e6d64"}, "replacement": "x"}
                },
                {
                    "kind": "explicit-target-missing", "finding_key": "sha256:c",
                    "description": "no span", "effective_disposition": "record",
                    "location": {"path": "docs/b.md", "span": null}, "fix": null
                }
            ]
        }
    }"#,
    )
    .unwrap();
    let log = super::log(&envelope);
    let bytes = serde_json::to_vec(&log).unwrap();
    assert_eq!(bytes, serde_json_canonicalizer::to_vec(&log).unwrap());
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let run = &value["runs"][0];
    let invocation = &run["invocations"][0];
    assert_eq!(invocation["executionSuccessful"], false);
    assert_eq!(invocation["exitCode"], 2);
    assert_eq!(
        invocation["toolExecutionNotifications"][0]["descriptor"]["id"],
        "READ_FAILED"
    );
    let first = &run["results"][0];
    assert_eq!(first["level"], "error");
    assert_eq!(first["ruleIndex"], 0);
    assert_eq!(first["message"]["text"], "missing \"target\"\n");
    assert_eq!(
        first["partialFingerprints"]["amissFindingKey/v1"],
        "sha256:a"
    );
    let location = &first["locations"][0]["physicalLocation"];
    assert_eq!(location["artifactLocation"]["uri"], "docs/a%20b.md");
    assert_eq!(
        location["region"],
        serde_json::json!({
            "endColumn": 8, "endLine": 2, "startColumn": 3, "startLine": 2
        })
    );
    let replacement = &first["fixes"][0]["artifactChanges"][0]["replacements"][0];
    assert_eq!(
        replacement["deletedRegion"],
        serde_json::json!({"byteLength": 3, "byteOffset": 4})
    );
    assert_eq!(replacement["insertedContent"]["text"], "");
    let unknown = &run["results"][1];
    assert_eq!(unknown["level"], "warning");
    for omitted in ["ruleIndex", "locations", "fixes"] {
        assert!(unknown.get(omitted).is_none(), "{omitted}");
    }
    let unlocated = &run["results"][2];
    assert_eq!(unlocated["level"], "note");
    assert!(
        unlocated["locations"][0]["physicalLocation"]
            .get("region")
            .is_none()
    );
    assert!(unlocated.get("fixes").is_none());
}
