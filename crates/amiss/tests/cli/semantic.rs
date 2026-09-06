#![expect(
    clippy::unwrap_used,
    reason = "integration assertions over self-created repositories and JSON"
)]

use std::{fs, process::Command};

use tempfile::TempDir;

use crate::support::{amiss, payload};

fn fixture() -> (amiss_fixtures::CommitPair, String) {
    let policy = serde_json::json!({
        "schema": "amiss/scanner-policy",
        "document_includes": [],
        "projection_assertions": [
            {
                "document": "api.md",
                "name": "public-api",
                "projection": "sorted-rows-v1",
                "sink": "previous-code",
                "source": {
                    "kind": "record-set",
                    "set": "rust/example/local-function-declarations",
                },
            },
            {
                "document": "function.md",
                "name": "check-signature",
                "projection": "code-text-v1",
                "sink": "previous-code",
                "source": {
                    "kind": "record-value",
                    "set": "rust/example/local-function-declarations",
                    "key": "fn/example::check",
                },
            },
        ],
        "protected_inventory": [],
        "finding_dispositions": [],
    });
    let policy = serde_json::to_string(&policy).unwrap();
    let fixture = amiss_fixtures::commit_pair(
        &[("README.md", "# Base\n")],
        &[
            (
                "api.md",
                "```text\npub fn example::check() -> bool\npub fn example::render() -> String\n```\n[amiss:public-api]: <amiss:projection>\n",
            ),
            (
                "function.md",
                "```text\npub fn example::check() -> bool\n```\n[amiss:check-signature]: <amiss:projection>\n",
            ),
            (".amiss/scanner-policy.json", &policy),
        ],
    )
    .unwrap();
    let input_path = fixture.root().join("public-api-records.json");
    let input = serde_json::json!({
        "schema": "amiss/record-set-input",
        "producer_identity": "amiss-rust-public-api",
        "context_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "input_digest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "complete": true,
        "name": "rust/example/local-function-declarations",
        "records": [
            {
                "key": "fn/example::check",
                "value": "pub fn example::check() -> bool",
            },
            {
                "key": "fn/example::render",
                "value": "pub fn example::render() -> String",
            },
        ],
    });
    fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
    let input_path = amiss_fixtures::path_arg(&input_path);
    let (code, stdout, stderr) = amiss(&["record-set", "--evidence", &input_path]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    let parsed = amiss_wire::json::parse(&stdout).unwrap();
    let mut canonical = serde_json_canonicalizer::to_vec(&parsed).unwrap();
    canonical.push(b'\n');
    assert_eq!(
        stdout, canonical,
        "record-set output is canonical JSON plus LF"
    );
    amiss_wire::semantic::parse_template(&stdout).unwrap();

    let template_path = fixture.root().join("public-api.json");
    fs::write(&template_path, stdout).unwrap();
    (fixture, amiss_fixtures::path_arg(&template_path))
}

#[test]
fn rust_api_template_projects_one_declaration_and_the_complete_set() {
    let (fixture, template) = fixture();
    let commit_selector = ["--candidate", fixture.candidate.as_str()];
    let index_selector = ["--index"];
    for (base, selector) in [
        (fixture.base.as_str(), commit_selector.as_slice()),
        (fixture.candidate.as_str(), index_selector.as_slice()),
    ] {
        let mut args = vec![
            "check",
            "--repo",
            &fixture.repo,
            "--object-format",
            "sha1",
            "--base",
            base,
        ];
        args.extend_from_slice(selector);
        args.extend([
            "--profile",
            "observe",
            "--semantic-template",
            &template,
            "--format",
            "json",
        ]);
        let (code, stdout, stderr) = amiss(&args);
        assert_eq!((code, stderr.as_str()), (0, ""));
        let body = payload(&stdout);
        assert_eq!(body["result"]["complete"], true);
        assert_eq!(body["errors"], serde_json::json!([]));
        assert_eq!(body["findings"], serde_json::json!([]));
        assert_eq!(body["controls"]["sandbox"]["assurance"], "self-asserted");
        assert_eq!(
            body["controls"]["semantic_evidence"][0]["producer"]["identity"],
            "amiss-rust-public-api"
        );
    }
}

#[test]
fn a_template_cannot_choose_its_candidate() {
    let (fixture, _valid_template) = fixture();
    let path = fixture.root().join("invalid-template.json");
    let bytes = br#"{
      "schema":"amiss/semantic-evidence-template",
      "producer":{
        "kind":"record-set",
        "identity":"test-public-api",
        "version":"1",
        "context_digest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "input_digest":"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
      },
      "complete":true,
      "observations":[],
      "candidate_identity_digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }"#;
    fs::write(&path, bytes).unwrap();
    let path = amiss_fixtures::path_arg(&path);
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fixture.repo,
        "--object-format",
        "sha1",
        "--base",
        &fixture.base,
        "--candidate",
        &fixture.candidate,
        "--profile",
        "observe",
        "--semantic-template",
        &path,
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (2, ""));
    let body = payload(&stdout);
    assert_eq!(body["result"]["complete"], false);
    assert_eq!(body["errors"][0]["code"], "UNKNOWN_FIELD");
}

#[test]
fn record_set_authoring_refuses_noncanonical_specialist_rows_without_a_repository() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("records.json");
    let input = serde_json::json!({
        "schema": "amiss/record-set-input",
        "producer_identity": "test-public-api",
        "context_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "input_digest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "complete": true,
        "name": "rust/public-api",
        "records": [
            {"key": "z", "value": "Z"},
            {"key": "a", "value": "A"},
        ],
    });
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .current_dir(directory.path())
        .args(["record-set", "--evidence"])
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "amiss record-set: set is not sorted at $.records\n"
    );
}
