use std::{fs, process::Command};

use amiss_wire::{
    digest::hb,
    report::{
        PAYLOAD_SCHEMA,
        model::{RepoPath, RepoPathBytes, ReportEnvelope},
    },
};
use serde_json::{Value, json};

#[test]
fn refs_preserve_original_occurrences_but_reject_unknown_span_fields() {
    let mut report: ReportEnvelope =
        serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let mut comparison = report.payload.observations[0].clone();
    let candidate = comparison.candidate.as_ref().unwrap();
    let mut alternative = candidate.clone();
    alternative.document = RepoPath::Bytes(RepoPathBytes {
        bytes_hex: hex::encode(b"docs/\xff.md"),
    });
    let expected = [candidate.clone(), alternative.clone()];
    comparison.alternatives.candidate = vec![alternative.clone()];
    report.payload.observations = vec![comparison];
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.json");
    report.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
    );
    fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(["refs", "--report"])
        .arg(&path)
        .args(["--target", "docs/guide.md", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    let mut expected_bytes = serde_json_canonicalizer::to_vec(&expected).unwrap();
    expected_bytes.push(b'\n');
    assert_eq!(output.stdout, expected_bytes);

    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(["refs", "--report"])
        .arg(&path)
        .args(["--target", "docs/guide.md"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(human.contains("candidate occurrences 2"), "{human}");
    assert!(
        human.contains(r#"reference "docs/\u00ff.md":3:9"#),
        "{human}"
    );

    report.payload.observations[0].candidate = None;
    report.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
    );
    fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(["refs", "--report"])
        .arg(&path)
        .args(["--target", "docs/guide.md", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let mut expected_bytes = serde_json_canonicalizer::to_vec(&[alternative]).unwrap();
    expected_bytes.push(b'\n');
    assert_eq!(output.stdout, expected_bytes);

    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    let invalid = payload.replacen(
        "\"source_span\":{",
        "\"source_span\":{\"__unexpected\":true,",
        1,
    );
    assert_ne!(payload, invalid);
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&report).unwrap()).unwrap();
    let wire = wire.replace(&payload, &invalid).replace(
        &report.payload_digest.to_string(),
        &hb(PAYLOAD_SCHEMA, invalid.as_bytes()).to_string(),
    );
    fs::write(&path, wire).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(["refs", "--report"])
        .arg(&path)
        .args(["--target", "docs/guide.md", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{:?}", output.stderr);
}

#[test]
fn refs_query_each_path_source_and_raw_byte_targets() {
    let original: Value = serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.json");
    for (intent, resolution, flag, target) in [
        (
            json!("docs/query.md"),
            json!({"kind": "missing", "reason": "label-not-declared"}),
            "--target",
            "docs/query.md",
        ),
        (
            Value::Null,
            json!({"kind": "missing", "reason": "path-not-found", "path": "docs/query.md", "near": null}),
            "--target",
            "docs/query.md",
        ),
        (
            Value::Null,
            json!({"kind": "resolved", "target": {"kind": "tree", "path": "docs/query.md"}}),
            "--target",
            "docs/query.md",
        ),
        (
            Value::Null,
            json!({"kind": "unsupported-version", "scope": {"kind": "known-path", "path": "docs/query.md"}}),
            "--target",
            "docs/query.md",
        ),
        (
            json!({"bytes_hex": "646f63732fff2e6d64"}),
            json!({"kind": "missing", "reason": "label-not-declared"}),
            "--target-bytes-hex",
            "646f63732fff2e6d64",
        ),
    ] {
        let mut report = original.clone();
        let mut comparison = report["payload"]["observations"][0].clone();
        comparison["candidate"]["intent"]["repository_path"] = intent;
        comparison["candidate"]["resolution"] = resolution;
        let expected = json!([comparison["candidate"]]);
        report["payload"]["observations"] = json!([comparison]);
        bind(&mut report, &path).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
            .args(["refs", "--report"])
            .arg(&path)
            .args([flag, target, "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            expected
        );
    }
}

fn bind(report: &mut Value, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json_canonicalizer::to_vec(&report["payload"])?;
    report["payload_digest"] = json!(hb(PAYLOAD_SCHEMA, &payload));
    fs::write(path, serde_json::to_vec_pretty(report)?)?;
    Ok(())
}
