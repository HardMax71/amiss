#![expect(
    clippy::unwrap_used,
    reason = "integration assertions over self-created repositories and JSON"
)]

use std::fs;

use crate::support::{amiss, payload};

fn fixture() -> (amiss_fixtures::CommitPair, String) {
    let policy = serde_json::json!({
        "schema": "amiss/scanner-policy",
        "document_includes": [],
        "projection_assertions": [{
            "document": "docs.md",
            "name": "public-api",
            "projection": "code-text-v1",
            "sink": "previous-code",
            "source": {
                "kind": "record-value",
                "set": "rust/public-api",
                "key": "amiss::check",
            },
        }],
        "protected_inventory": [],
        "finding_dispositions": [],
    });
    let policy = serde_json::to_string(&policy).unwrap();
    let fixture = amiss_fixtures::commit_pair(
        &[("README.md", "# Base\n")],
        &[
            (
                "docs.md",
                "```text\npub fn check()\n```\n[amiss:public-api]: <amiss:projection>\n",
            ),
            (".amiss/scanner-policy.json", &policy),
        ],
    )
    .unwrap();
    let path = fixture.root().join("public-api.json");
    let template = serde_json::json!({
        "schema": "amiss/semantic-evidence-template",
        "producer": {
            "kind": "record-set",
            "identity": "test-public-api",
            "version": "1",
            "context_digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "input_digest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        },
        "complete": true,
        "observations": [{
            "kind": "record-set",
            "name": "rust/public-api",
            "records": [{
                "key": "amiss::check",
                "value": "pub fn check()",
            }],
        }],
    });
    fs::write(&path, serde_json::to_vec(&template).unwrap()).unwrap();
    (fixture, amiss_fixtures::path_arg(&path))
}

#[test]
fn one_self_asserted_template_binds_to_commit_and_index_candidates() {
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
        assert!(
            body["findings"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["kind"] != "projection-drift")),
            "the bound record proves the visible projection: {body}"
        );
        assert_eq!(body["controls"]["sandbox"]["assurance"], "self-asserted");
        assert_eq!(
            body["controls"]["semantic_evidence"][0]["producer"]["identity"],
            "test-public-api"
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
