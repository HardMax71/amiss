#![cfg(unix)]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use amiss_scan::report::{CandidateBlock, RequestDigests, candidate_identity_digest};
use amiss_scan::{Effects, Setup, SnapshotIdentity};
use amiss_wire::controls::OrganizationFloor;
use amiss_wire::digest::hb;
use amiss_wire::report::EngineProvenance;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git output utf-8")
}

struct Fixture {
    dir: TempDir,
    base: String,
    candidate: String,
    base_tree: String,
    candidate_tree: String,
}

impl Fixture {
    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn request(&self, name: &str, body: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        fs::write(&path, body).unwrap();
        path
    }
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "see [note](note.md)\n").unwrap();
    fs::write(root.join("note.md"), "[readme](README.md)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("note.md"), "[readme](README.md) updated\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let base = git(root, &["rev-parse", "HEAD~1"]).trim().to_owned();
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let base_tree = git(root, &["rev-parse", "HEAD~1^{tree}"]).trim().to_owned();
    let candidate_tree = git(root, &["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    Fixture {
        dir,
        base,
        candidate,
        base_tree,
        candidate_tree,
    }
}

fn evaluation_request(fx: &Fixture, with_identity: bool) -> String {
    let identity = if with_identity {
        r#"{ "host": "github.com", "owner": "acme", "name": "docs" }"#.to_owned()
    } else {
        "null".to_owned()
    };
    let reference = if with_identity {
        "\"refs/heads/main\"".to_owned()
    } else {
        "null".to_owned()
    };
    format!(
        r#"{{
  "schema": "amiss/scanner-evaluation-request/v1",
  "profile": "enforce",
  "mode": "commit-pair",
  "object_format": "sha1",
  "repository": {identity},
  "ref": {reference},
  "default_branch_ref": {reference},
  "base_commit_oid": "{base}",
  "candidate_commit_oid": "{candidate}"
}}"#,
        base = fx.base,
        candidate = fx.candidate,
    )
}

const SNAPSHOT_REQUEST: &str = r#"{
  "schema": "amiss/scanner-snapshot-request/v1",
  "materialization": "git-objects",
  "repository_handle": 3,
  "pre_acquired": true
}"#;

const EMPTY_CONTROLS: &str = r#"{
  "schema": "amiss/scanner-controls-request/v1",
  "organization_floor": null,
  "debt_snapshot": null,
  "waiver_bundle": null,
  "trusted_time": null,
  "execution_constraint": null
}"#;

const FLOOR_VALUE: &str = r#"{
      "schema": "amiss/organization-floor/v1",
      "floor_id": "acme/scanner-floor-2026-07",
      "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
      "ref": "refs/heads/main",
      "minimum_profile": "observe",
      "minimum_dispositions": [],
      "protected_inventory": [],
      "protected_control_paths": [],
      "waivable_finding_kinds": [],
      "authorized_debt_owners": [],
      "authorized_waiver_issuers": [],
      "resource_limits": []
    }"#;

fn run_wrapper(
    fx: &Fixture,
    evaluation: &Path,
    snapshot: &Path,
    controls: &Path,
) -> (i32, Vec<u8>) {
    let output = Command::new(env!("CARGO_BIN_EXE_amiss-wrapper"))
        .arg("check")
        .arg("--repository")
        .arg(fx.root())
        .arg("--evaluation-request")
        .arg(evaluation)
        .arg("--snapshot-request")
        .arg(snapshot)
        .arg("--controls-request")
        .arg(controls)
        .output()
        .expect("run amiss-wrapper");
    (output.status.code().expect("exit code"), output.stdout)
}

fn validated(wire: &[u8]) -> serde_json::Value {
    let envelope: serde_json::Value = serde_json::from_slice(wire).unwrap();
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/scanner-report-v1.schema.json"),
    )
    .unwrap()
    .replace("assure/", "amiss/")
    .replace(".assure/", ".amiss/")
    .replace("assure-action-bootstrap-v1", "amiss-action-bootstrap-v1");
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema_json).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path))
        .collect();
    assert_eq!(defects, Vec::<String>::new(), "schema-clean report");
    envelope["payload"].clone()
}

#[test]
fn a_clean_run_passes_through_the_wire() {
    let fx = fixture();
    let evaluation = fx.request("evaluation.json", &evaluation_request(&fx, false));
    let snapshot = fx.request("snapshot.json", SNAPSHOT_REQUEST);
    let controls = fx.request("controls.json", EMPTY_CONTROLS);
    let (code, wire) = run_wrapper(&fx, &evaluation, &snapshot, &controls);
    let payload = validated(&wire);

    assert_eq!(code, 0);
    assert_eq!(payload["result"]["status"], "pass");
    assert_eq!(payload["evaluation"]["mode"], "commit-pair");
    assert_eq!(
        payload["evaluation"]["base"]["commit_oid"],
        serde_json::Value::String(fx.base.clone())
    );
    assert_eq!(payload["controls"]["organization_floor"]["status"], "none");
}

#[test]
fn verified_controls_flow_through_the_wire() {
    let fx = fixture();
    let floor_digest = OrganizationFloor::parse(FLOOR_VALUE.as_bytes())
        .map_err(|defect| format!("{defect:?}"))
        .unwrap()
        .digest
        .to_string();
    let identity_setup = Setup {
        engine: EngineProvenance {
            version: "0.0.0-test".to_owned(),
            digest: hb("amiss/scanner-engine/v1", b"irrelevant"),
        },
        enforce: true,
        repository: Some(("acme".to_owned(), "docs".to_owned())),
        candidate_ref: Some("refs/heads/main".to_owned()),
        default_branch_ref: Some("refs/heads/main".to_owned()),
        base: SnapshotIdentity {
            object_format: "sha1",
            commit_oid: fx.base.clone(),
            tree_oid: fx.base_tree.clone(),
        },
        candidate: CandidateBlock::Commit(SnapshotIdentity {
            object_format: "sha1",
            commit_oid: fx.candidate.clone(),
            tree_oid: fx.candidate_tree.clone(),
        }),
        policy: Effects::default(),
        controls_unavailable: None,
        requests: RequestDigests::default(),
    };
    let identity_digest = candidate_identity_digest(&identity_setup);
    let statement = format!(
        r#"{{
      "schema": "amiss/scanner-trusted-time-statement/v1",
      "controller": "github-actions-required-workflow-clock-v1",
      "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
      "ref": "refs/heads/main",
      "candidate_identity_digest": "{identity_digest}",
      "provider_run_id": "987654321",
      "provider_run_attempt": 2,
      "evaluation_instant": "2026-07-12T10:00:00Z",
      "valid_until": "2026-07-12T10:09:00Z"
    }}"#
    );
    let statement_digest = amiss_wire::controls::TrustedTimeStatement::parse(statement.as_bytes())
        .unwrap()
        .digest
        .to_string();
    let controls_body = format!(
        r#"{{
  "schema": "amiss/scanner-controls-request/v1",
  "organization_floor": {{
    "value": {FLOOR_VALUE},
    "expected_digest": "{floor_digest}",
    "trust_source": "organization-ruleset"
  }},
  "debt_snapshot": null,
  "waiver_bundle": null,
  "trusted_time": {{
    "value": {statement},
    "expected_digest": "{statement_digest}",
    "provider_run_id": "987654321",
    "provider_run_attempt": 2
  }},
  "execution_constraint": null
}}"#
    );
    let evaluation = fx.request("evaluation.json", &evaluation_request(&fx, true));
    let snapshot = fx.request("snapshot.json", SNAPSHOT_REQUEST);
    let controls = fx.request("controls.json", &controls_body);
    let (code, wire) = run_wrapper(&fx, &evaluation, &snapshot, &controls);
    let payload = validated(&wire);

    assert_eq!(code, 0, "clean repository under a verified floor passes");
    assert_eq!(
        payload["controls"]["organization_floor"]["status"],
        "verified"
    );
    assert_eq!(
        payload["controls"]["organization_floor"]["digest"],
        serde_json::Value::String(floor_digest)
    );
    assert_eq!(
        payload["controls"]["trusted_time_source"]["status"],
        "verified"
    );
    assert_eq!(payload["evaluation"]["trusted_time"], true);
    assert_eq!(
        payload["evaluation"]["evaluation_instant"],
        "2026-07-12T10:00:00Z"
    );
}

#[test]
fn an_expected_digest_mismatch_is_fatal() {
    let fx = fixture();
    let wrong = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let controls_body = format!(
        r#"{{
  "schema": "amiss/scanner-controls-request/v1",
  "organization_floor": {{
    "value": {FLOOR_VALUE},
    "expected_digest": "{wrong}",
    "trust_source": "organization-ruleset"
  }},
  "debt_snapshot": null,
  "waiver_bundle": null,
  "trusted_time": null,
  "execution_constraint": null
}}"#
    );
    let evaluation = fx.request("evaluation.json", &evaluation_request(&fx, true));
    let snapshot = fx.request("snapshot.json", SNAPSHOT_REQUEST);
    let controls = fx.request("controls.json", &controls_body);
    let (code, wire) = run_wrapper(&fx, &evaluation, &snapshot, &controls);
    let payload = validated(&wire);

    assert_eq!(code, 2);
    assert_eq!(payload["result"]["status"], "incomplete");
    assert_eq!(payload["controls"]["status"], "unavailable");
    assert_eq!(
        payload["controls"]["reasons"],
        serde_json::json!(["invalid-external-control"])
    );
    assert!(
        payload["controls"]["request_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "the completely captured controls stream keeps its digest"
    );
    assert!(
        payload["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "DIGEST_MISMATCH")
    );
    assert_eq!(
        payload["evaluation"]["base"]["commit_oid"],
        serde_json::Value::String(fx.base.clone()),
        "the defect settles against resolved snapshot identities"
    );
}

#[test]
fn a_malformed_request_is_unreadable_with_its_digest() {
    let fx = fixture();
    let evaluation = fx.request("evaluation.json", &evaluation_request(&fx, false));
    let snapshot = fx.request("snapshot.json", SNAPSHOT_REQUEST);
    let controls = fx.request("controls.json", "{ not json");
    let (code, wire) = run_wrapper(&fx, &evaluation, &snapshot, &controls);
    let payload = validated(&wire);

    assert_eq!(code, 2);
    assert_eq!(payload["evaluation"]["status"], "unavailable");
    assert_eq!(
        payload["evaluation"]["reasons"],
        serde_json::json!(["request-unreadable"])
    );
    assert!(
        payload["evaluation"]["request_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "the evaluation stream captured completely"
    );
    assert!(
        payload["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "REQUEST_UNREADABLE")
    );
}

#[test]
fn an_oversize_request_stream_has_no_digest() {
    let fx = fixture();
    let oversize = " ".repeat(16_777_217);
    let evaluation = fx.request("evaluation.json", &oversize);
    let snapshot = fx.request("snapshot.json", SNAPSHOT_REQUEST);
    let controls = fx.request("controls.json", EMPTY_CONTROLS);
    let (code, wire) = run_wrapper(&fx, &evaluation, &snapshot, &controls);
    let payload = validated(&wire);

    assert_eq!(code, 2);
    assert_eq!(payload["evaluation"]["status"], "unavailable");
    assert_eq!(
        payload["evaluation"]["request_digest"],
        serde_json::Value::Null,
        "EOF was not obtained within the cap"
    );
    assert_eq!(
        payload["controls"]["request_digest"]
            .as_str()
            .map(|digest| digest.starts_with("sha256:")),
        Some(true)
    );
}

#[test]
fn mismatched_mode_pairing_is_an_invalid_invocation() {
    let fx = fixture();
    let evaluation = fx.request("evaluation.json", &evaluation_request(&fx, false));
    let snapshot = fx.request(
        "snapshot.json",
        &SNAPSHOT_REQUEST.replace("git-objects", "index"),
    );
    let controls = fx.request("controls.json", EMPTY_CONTROLS);
    let (code, wire) = run_wrapper(&fx, &evaluation, &snapshot, &controls);
    let payload = validated(&wire);

    assert_eq!(code, 2);
    assert_eq!(
        payload["evaluation"]["reasons"],
        serde_json::json!(["invalid-invocation"])
    );
    assert!(
        payload["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["code"] == "INVALID_INVOCATION")
    );
}

#[test]
fn usage_defects_reject_the_invocation() {
    let fx = fixture();
    let evaluation = fx.request("evaluation.json", &evaluation_request(&fx, false));
    let output = Command::new(env!("CARGO_BIN_EXE_amiss-wrapper"))
        .arg("check")
        .arg("--repository")
        .arg(fx.root())
        .arg("--evaluation-request")
        .arg(&evaluation)
        .output()
        .expect("run amiss-wrapper");
    assert_eq!(output.status.code(), Some(2));
    let payload = validated(&output.stdout);
    assert_eq!(
        payload["evaluation"]["reasons"],
        serde_json::json!(["invalid-invocation"])
    );
}

#[test]
fn index_mode_runs_through_the_wire() {
    let fx = fixture();
    let evaluation_body = evaluation_request(&fx, false)
        .replace("\"commit-pair\"", "\"index\"")
        .replace(&format!("\"{}\"", fx.candidate), "null");
    let evaluation = fx.request("evaluation.json", &evaluation_body);
    let snapshot = fx.request(
        "snapshot.json",
        &SNAPSHOT_REQUEST.replace("git-objects", "index"),
    );
    let controls = fx.request("controls.json", EMPTY_CONTROLS);
    let (code, wire) = run_wrapper(&fx, &evaluation, &snapshot, &controls);
    let payload = validated(&wire);

    assert_eq!(code, 0);
    assert_eq!(payload["evaluation"]["mode"], "index");
    assert_eq!(payload["evaluation"]["candidate"]["kind"], "index");
}

#[test]
fn the_output_flag_writes_the_accepted_envelope() {
    let fx = fixture();
    let evaluation = fx.request("evaluation.json", &evaluation_request(&fx, false));
    let snapshot = fx.request("snapshot.json", SNAPSHOT_REQUEST);
    let controls = fx.request("controls.json", EMPTY_CONTROLS);
    let out = fx.root().join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_amiss-wrapper"))
        .arg("check")
        .arg("--repository")
        .arg(fx.root())
        .arg("--evaluation-request")
        .arg(&evaluation)
        .arg("--snapshot-request")
        .arg(&snapshot)
        .arg("--controls-request")
        .arg(&controls)
        .arg("--output")
        .arg(&out)
        .output()
        .expect("run amiss-wrapper");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty(), "the envelope goes to the file");
    let written = fs::read(&out).unwrap();
    let payload = validated(&written);
    assert_eq!(payload["result"]["status"], "pass");
}

/// The frozen dossier examples: the indented readable envelope and its exact
/// one-line `JCS(envelope) || LF` canonicalization.
fn dossier_example(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/spec/examples")
            .join(name),
    )
    .unwrap()
}

fn foreign_expectations() -> amiss_wrapper::Expectations {
    amiss_wrapper::Expectations {
        engine_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .to_owned(),
        base_commit: "0000000000000000000000000000000000000000".to_owned(),
        candidate_commit: None,
        floor_digest: None,
    }
}

#[test]
fn the_indented_example_is_rejected_as_noncanonical() {
    let indented = dossier_example("scanner-report-v1.json");
    assert_eq!(
        amiss_wrapper::accept(&indented, &foreign_expectations()),
        Err(amiss_wrapper::AcceptanceDefect::Noncanonical),
        "a readable parsed-value example is not a valid emitted byte fixture"
    );
}

#[test]
fn the_canonical_golden_is_the_canonicalization_of_the_indented_value() {
    let indented = dossier_example("scanner-report-v1.json");
    let golden = dossier_example("scanner-report-v1.canonical.json");
    let parsed = amiss_wire::json::parse(&indented).unwrap();
    let mut recanonicalized = amiss_wire::json::canonical(&parsed);
    recanonicalized.push(b'\n');
    assert_eq!(
        recanonicalized, golden,
        "the smoke-checker equivalence holds under this serializer"
    );
}

#[test]
fn the_canonical_golden_clears_the_canonicality_gate() {
    let golden = dossier_example("scanner-report-v1.canonical.json");
    let defect = amiss_wrapper::accept(&golden, &foreign_expectations()).unwrap_err();
    assert_ne!(
        defect,
        amiss_wrapper::AcceptanceDefect::Noncanonical,
        "the exact one-line golden is canonical"
    );
    assert_eq!(
        defect,
        amiss_wrapper::AcceptanceDefect::PayloadDigest,
        "the frozen example's digest lives in the research namespace"
    );
}
