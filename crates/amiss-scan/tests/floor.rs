use std::fs;
use std::path::Path;
use std::process::Command;

use amiss_fixtures::stage_symlink;
use amiss_git::Repository;
use amiss_scan::SetupShell;
use amiss_scan::pipeline::commit_pair;
use amiss_scan::policy::{FloorInput, TrustSource, verify_floor};
use amiss_wire::controls::OrganizationFloor;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use tempfile::TempDir;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
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

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine/v1", b"test engine"),
    }
}

fn floor_json(extra: &str) -> String {
    format!(
        r#"{{
  "schema": "amiss/organization-floor/v1",
  "floor_id": "acme/scanner-floor-2026-07",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  {extra}
}}"#
    )
}

const EMPTY_ARRAYS: &str = r#"  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [],
  "authorized_waiver_issuers": [],
  "resource_limits": []"#;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn floor_input(extra: &str) -> FloorInput {
    FloorInput {
        floor: OrganizationFloor::parse(floor_json(extra).as_bytes()).unwrap(),
        trust_source: TrustSource::ExternalRequiredWorkflow,
    }
}

fn shell(floor: Option<FloorInput>) -> SetupShell {
    SetupShell {
        engine: engine(),
        enforce: false,
        repository: Some(("acme".to_owned(), "docs".to_owned())),
        candidate_ref: Some("refs/heads/main".to_owned()),
        default_branch_ref: None,
        floor,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    }
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn two_commits(root: &Path) -> (Repository, Oid, Oid) {
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let base = git(root, &["rev-parse", "HEAD~1"]).trim().to_owned();
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    (
        repo,
        Oid::new(ObjectFormat::Sha1, base).unwrap(),
        Oid::new(ObjectFormat::Sha1, candidate).unwrap(),
    )
}

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test fixture helper"
)]
fn payload(
    setup: &SetupShell,
    repo: &Repository,
    base: &Oid,
    candidate: &Oid,
) -> serde_json::Value {
    let built = commit_pair(repo, &engine(), None, setup, base, candidate);
    let envelope: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/scanner-report-v1.schema.json"),
    )
    .unwrap()
    .replace("assure/", "amiss/")
    .replace(".assure/", ".amiss/");
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema_json).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path))
        .collect();
    assert_eq!(defects, Vec::<String>::new(), "schema-clean report");
    let mut value = envelope["payload"].clone();
    value["exit_code"] = serde_json::Value::from(built.exit_code);
    value
}

#[test]
fn the_floor_binding_is_repository_ref_and_profile_equality() {
    let input = floor_input(EMPTY_ARRAYS);
    let repository = Some(("acme", "docs"));
    let matching = verify_floor(&input, repository, Some("refs/heads/main"), false);
    assert!(matching.is_ok());

    assert!(
        verify_floor(
            &input,
            Some(("acme", "other")),
            Some("refs/heads/main"),
            false
        )
        .is_err()
    );
    assert!(verify_floor(&input, None, Some("refs/heads/main"), false).is_err());
    assert!(verify_floor(&input, repository, Some("refs/heads/dev"), false).is_err());
    assert!(verify_floor(&input, repository, None, false).is_err());

    let strict = FloorInput {
        floor: OrganizationFloor::parse(
            floor_json(EMPTY_ARRAYS)
                .replace("\"observe\"", "\"enforce\"")
                .as_bytes(),
        )
        .map_err(|defect| format!("{defect:?}"))
        .unwrap(),
        trust_source: TrustSource::OrganizationRuleset,
    };
    assert!(verify_floor(&strict, repository, Some("refs/heads/main"), false).is_err());
    assert!(verify_floor(&strict, repository, Some("refs/heads/main"), true).is_ok());
}

#[test]
fn a_mismatched_floor_makes_controls_unavailable_with_real_identities() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("README.md"), "candidate\n").unwrap();
    let (repo, base, candidate) = two_commits(root);

    let mut setup = shell(Some(floor_input(EMPTY_ARRAYS)));
    setup.candidate_ref = Some("refs/heads/dev".to_owned());
    let report = payload(&setup, &repo, &base, &candidate);

    assert_eq!(report["exit_code"], 2);
    assert_eq!(report["result"]["status"], "incomplete");
    assert_eq!(report["controls"]["status"], "unavailable");
    assert_eq!(
        report["controls"]["reasons"],
        serde_json::json!(["control-binding-mismatch"])
    );
    let errors = report["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["code"], "CONTROL_BINDING_MISMATCH");
    assert_eq!(errors[0]["phase"], "configuration");
    assert_eq!(errors[0]["path"], serde_json::Value::Null);
    assert_eq!(
        report["evaluation"]["base"]["commit_oid"],
        serde_json::Value::String(base.as_str().to_owned()),
        "the mismatch projection still carries the resolved snapshot identity"
    );
}

#[test]
fn a_verified_floor_raises_dispositions_and_discloses_provenance() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "clean\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("README.md"), "see [gone](missing.md)\n").unwrap();
    let (repo, base, candidate) = two_commits(root);

    let extra = EMPTY_ARRAYS.replace(
        "\"minimum_dispositions\": []",
        "\"minimum_dispositions\": [ { \"finding_kind\": \"explicit-target-missing\", \"disposition\": \"fail\" } ]",
    );
    let input = floor_input(&extra);
    let floor_digest = input.floor.digest.to_string();
    let report = payload(&shell(Some(input)), &repo, &base, &candidate);

    let provenance = &report["controls"]["organization_floor"];
    assert_eq!(provenance["status"], "verified");
    assert_eq!(
        provenance["digest"],
        serde_json::Value::String(floor_digest)
    );
    assert_eq!(provenance["trust_source"], "external-required-workflow");

    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(finding["configured_disposition"], "fail");
    assert_eq!(finding["effective_disposition"], "fail");
    let trace = finding["policy_trace"].as_array().unwrap();
    assert_eq!(trace.len(), 2);
    assert_eq!(trace[0]["source"], "built-in");
    assert_eq!(trace[0]["before"], "record");
    assert_eq!(trace[0]["after"], "warn");
    assert_eq!(trace[1]["source"], "organization-floor");
    assert_eq!(trace[1]["rule_id"], "floor/explicit-target-missing");
    assert_eq!(trace[1]["before"], "warn");
    assert_eq!(trace[1]["after"], "fail");

    assert_eq!(report["result"]["status"], "fail");
    assert_eq!(report["exit_code"], 1);
}

#[test]
fn floor_inventory_and_protected_paths_emit_control_findings() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join(".github/workflows")).unwrap();
    fs::write(root.join("README.md"), "stable\n").unwrap();
    fs::write(root.join(".github/workflows/scan.yml"), "on: push\n").unwrap();
    fs::write(root.join("assets.bin"), [0_u8, 1, 2]).unwrap();
    git(root, &["add", "."]);
    stage_symlink(root, "README.md", "link.md").unwrap();
    git(root, &["commit", "-qm", "base"]);
    fs::write(
        root.join(".github/workflows/scan.yml"),
        "on: pull_request\n",
    )
    .unwrap();
    let (repo, base, candidate) = two_commits(root);

    let extra = EMPTY_ARRAYS
        .replace(
            "\"protected_inventory\": []",
            "\"protected_inventory\": [ \"assets.bin\", \"docs/required.md\" ]",
        )
        .replace(
            "\"protected_control_paths\": []",
            "\"protected_control_paths\": [ \".github/workflows/scan.yml\", \"README.md\", \"link.md\" ]",
        );
    let report = payload(&shell(Some(floor_input(&extra))), &repo, &base, &candidate);

    let rows: Vec<(String, String)> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "coverage-reduced" || row["kind"] == "control-plane-changed")
        .map(|row| {
            let scope = &row["key_input"]["scope"];
            (
                scope["rule_id"].as_str().unwrap().to_owned(),
                scope["control_path"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert!(rows.contains(&(
        "coverage/floor-inventory-missing".to_owned(),
        "docs/required.md".to_owned()
    )));
    assert!(rows.contains(&(
        "coverage/floor-inventory-outside".to_owned(),
        "assets.bin".to_owned()
    )));
    assert!(
        rows.contains(&(
            "control/protected-path".to_owned(),
            ".github/workflows/scan.yml".to_owned()
        )),
        "a changed protected blob is a control-plane change"
    );
    assert!(
        rows.contains(&("control/protected-path".to_owned(), "link.md".to_owned())),
        "a symlink protected path is never present"
    );
    assert!(
        !rows
            .iter()
            .any(|(rule, path)| rule == "control/protected-path" && path == "README.md"),
        "the same present descriptor on both sides emits nothing"
    );
}

#[test]
fn a_verified_floor_tightens_scan_ceilings() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("a.md"), "a\n").unwrap();
    fs::write(root.join("b.md"), "b\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("a.md"), "a changed\n").unwrap();
    let (repo, base, candidate) = two_commits(root);

    let extra = EMPTY_ARRAYS.replace(
        "\"resource_limits\": []",
        "\"resource_limits\": [ { \"resource\": \"documents-per-snapshot\", \"maximum\": 1 } ]",
    );
    let report = payload(&shell(Some(floor_input(&extra))), &repo, &base, &candidate);

    assert_eq!(report["exit_code"], 2);
    assert_eq!(report["result"]["status"], "incomplete");
    let crossing = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["resource"] == "documents-per-snapshot")
        .unwrap();
    assert_eq!(crossing["code"], "RESOURCE_LIMIT_EXCEEDED");
    assert_eq!(crossing["configured_limit"], 1);
    assert_eq!(crossing["observed_lower_bound"], 2);
}

#[test]
fn a_verified_floor_tightens_the_policy_entry_budget() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{
  "schema": "amiss/scanner-policy/v1",
  "document_includes": [
    { "path": "docs/a.rst", "kind": "document" },
    { "path": "docs/b.rst", "kind": "document" }
  ],
  "protected_inventory": [ "README.md" ],
  "finding_dispositions": []
}"#,
    )
    .unwrap();
    let (repo, base, candidate) = two_commits(root);

    let extra = EMPTY_ARRAYS.replace(
        "\"resource_limits\": []",
        "\"resource_limits\": [ { \"resource\": \"repository-policy-entries\", \"maximum\": 2 } ]",
    );
    let report = payload(&shell(Some(floor_input(&extra))), &repo, &base, &candidate);

    assert_eq!(report["exit_code"], 2);
    assert_eq!(report["controls"]["status"], "unavailable");
    assert_eq!(
        report["controls"]["reasons"],
        serde_json::json!(["not-parsed"]),
        "a resource crossing has no configuration-invalid anchor"
    );
    let crossing = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["resource"] == "repository-policy-entries")
        .unwrap();
    assert_eq!(crossing["code"], "RESOURCE_LIMIT_EXCEEDED");
    assert_eq!(crossing["path"], ".amiss/scanner-policy.json");
    assert_eq!(crossing["configured_limit"], 2);
    assert_eq!(crossing["observed_lower_bound"], 3);
}

/// The complete-findings ceiling is the one array bound in the report with no
/// resource counter behind it until now: documents and observations snapshot
/// charged limits, and findings relied on arithmetic, every finding being too
/// heavy for 100,000 of them to fit under the 64 MiB wire cap. Arithmetic is a
/// property of today's finding shape, not a law, so the ceiling is now charged
/// against the exact array the report would ship, after control rows and
/// exceptions. A verified floor may tighten it like any other ceiling, which is
/// also what makes it testable without a hundred thousand findings: the run
/// calibrates itself, counting the findings the fixture produces unconstrained,
/// then setting the ceiling one below that and requiring the crossing to name
/// the resource, the limit, and the exact total, with no findings array left to
/// mistake for a truncated pass.
#[test]
fn a_verified_floor_tightens_the_complete_findings_ceiling() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("a.md"), "see [gone](missing.md)\n").unwrap();
    fs::write(root.join("b.md"), "b\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::write(root.join("a.md"), "see [gone](missing.md) still\n").unwrap();
    let (repo, base, candidate) = two_commits(root);

    let unconstrained = payload(
        &shell(Some(floor_input(EMPTY_ARRAYS))),
        &repo,
        &base,
        &candidate,
    );
    let produced = unconstrained["result"]["finding_count"].as_u64().unwrap();
    assert!(
        produced >= 2,
        "the fixture produces findings to count: {produced}"
    );

    let ceiling = produced - 1;
    let extra = EMPTY_ARRAYS.replace(
        "\"resource_limits\": []",
        &format!(
            "\"resource_limits\": [ {{ \"resource\": \"complete-findings\", \"maximum\": {ceiling} }} ]"
        ),
    );
    let report = payload(&shell(Some(floor_input(&extra))), &repo, &base, &candidate);

    assert_eq!(
        report["exit_code"], 2,
        "past the ceiling there is no result"
    );
    assert_eq!(report["result"]["status"], "incomplete");
    assert_eq!(report["result"]["complete"], false);
    let crossing = report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["resource"] == "complete-findings")
        .expect("the crossing names its resource");
    assert_eq!(crossing["code"], "RESOURCE_LIMIT_EXCEEDED");
    assert_eq!(crossing["configured_limit"], ceiling);
    assert_eq!(crossing["observed_lower_bound"], produced);
    assert!(
        report["findings"].as_array().unwrap().is_empty(),
        "an incomplete run publishes no findings to mistake for a truncated pass"
    );
}
