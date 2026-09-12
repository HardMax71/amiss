use sha2::Digest as _;
use std::{fs, process::Command};

use amiss_wire::{
    report::{
        PAYLOAD_SCHEMA,
        model::{
            MissingResolution, Occurrence, RepoPath, RepoPathBytes, ReportEnvelope, Resolution,
        },
    },
    resolution::{Target, VersionScope},
};

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
    bind(&mut report, &path).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args(["refs", "--report"])
        .arg(&path)
        .args(["--target", "docs/guide.md", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    assert_eq!(
        serde_json::from_slice::<Vec<Occurrence>>(&output.stdout)
            .unwrap()
            .len(),
        expected.len()
    );
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
    bind(&mut report, &path).unwrap();
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
        &amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(invalid.as_bytes())
                .finalize()
                .0,
        )
        .to_string(),
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
    let original: ReportEnvelope = serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report.json");
    let text_target = RepoPath::Text("docs/query.md".parse().unwrap());
    for (intent, resolution, flag, target) in [
        (
            Some(text_target.clone()),
            Resolution::Missing(MissingResolution::LabelNotDeclared {}),
            "--target",
            "docs/query.md",
        ),
        (
            None,
            Resolution::Missing(MissingResolution::PathNotFound {
                path: text_target.clone(),
                near: None,
                same_object_at: None,
            }),
            "--target",
            "docs/query.md",
        ),
        (
            None,
            Resolution::Resolved {
                target: Target::Tree {
                    path: text_target.clone(),
                },
            },
            "--target",
            "docs/query.md",
        ),
        (
            None,
            Resolution::UnsupportedVersion {
                scope: VersionScope::KnownPath { path: text_target },
            },
            "--target",
            "docs/query.md",
        ),
        (
            Some(RepoPath::Bytes(RepoPathBytes {
                bytes_hex: hex::encode(b"docs/\xff.md"),
            })),
            Resolution::Missing(MissingResolution::LabelNotDeclared {}),
            "--target-bytes-hex",
            "646f63732fff2e6d64",
        ),
    ] {
        let mut report = original.clone();
        let mut comparison = report.payload.observations[0].clone();
        let candidate = comparison.candidate.as_mut().unwrap();
        candidate.intent.repository_path = intent;
        candidate.resolution = resolution;
        let expected = [candidate.clone()];
        report.payload.observations = vec![comparison];
        bind(&mut report, &path).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
            .args(["refs", "--report"])
            .arg(&path)
            .args([flag, target, "--format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
        assert_eq!(
            serde_json::from_slice::<Vec<Occurrence>>(&output.stdout).unwrap(),
            expected
        );
    }
}

fn bind(
    report: &mut ReportEnvelope,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json_canonicalizer::to_vec(&report.payload)?;
    report.payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(&payload)
            .finalize()
            .0,
    );
    fs::write(path, serde_json::to_vec_pretty(report)?)?;
    Ok(())
}
