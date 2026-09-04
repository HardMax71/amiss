#![expect(clippy::unwrap_used, reason = "test fixture plumbing")]

use std::fs;

use amiss_fixtures::commit_chain;
use amiss_git::Repository;
use amiss_scan::pipeline::commit_pair;
use amiss_scan::policy::{DebtInput, FloorInput, TimeInput};
use amiss_scan::report::{CandidateBlock, Setup, SnapshotIdentity, candidate_identity_digest};
use amiss_wire::controls::{
    canonical_debt_snapshot, canonical_organization_floor, parse_debt_snapshot,
    parse_organization_floor, parse_trusted_time,
};
use amiss_wire::model::{BranchRef, ObjectFormat, Oid};
use amiss_wire::requests::RequestTrust;
use tempfile::TempDir;

use crate::support::amiss;

const FLOOR: &str = r#"{
  "schema": "amiss/organization-floor",
  "floor_id": "acme/scanner-floor-2026-08",
  "repository": { "host": "github.com", "owner": "acme", "name": "docs" },
  "ref": "refs/heads/main",
  "minimum_profile": "observe",
  "minimum_dispositions": [],
  "protected_inventory": [],
  "protected_control_paths": [],
  "waivable_finding_kinds": [],
  "authorized_debt_owners": [ "team:docs-platform" ],
  "authorized_waiver_issuers": [],
  "resource_limits": []
}"#;

struct Minted {
    chain: amiss_fixtures::CommitChain,
    root: String,
    base: String,
    candidate: String,
    output: TempDir,
}

fn floor_digest() -> String {
    let floor = parse_organization_floor(FLOOR.as_bytes()).unwrap();
    canonical_organization_floor(&floor).unwrap().1.to_string()
}

fn adopt_args(minted: &Minted, output: &str) -> Vec<String> {
    [
        "adopt",
        "--repo",
        &minted.root,
        "--object-format",
        "sha1",
        "--base",
        &minted.base,
        "--candidate",
        &minted.candidate,
        "--repository",
        "github.com/acme/docs",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
        "--floor-digest",
        &floor_digest(),
        "--debt-owner",
        "team:docs-platform",
        "--debt-reason",
        "adoption of the legacy tree",
        "--created-at",
        "2026-08-08T00:00:00Z",
        "--expires-at",
        "2027-08-08T00:00:00Z",
        "--debt-output",
        output,
    ]
    .iter()
    .map(|argument| (*argument).to_owned())
    .collect()
}

fn minted() -> (Minted, String) {
    let chain = commit_chain(&[
        ("base", &[("README.md", "see [gone](missing.md)\n")]),
        (
            "candidate",
            &[
                ("README.md", "see [gone](missing.md)\n"),
                ("note.md", "[readme](README.md)\n"),
            ],
        ),
    ])
    .unwrap();
    let output = TempDir::new().unwrap();
    let minted = Minted {
        root: chain.root().to_str().unwrap().to_owned(),
        base: chain.commits.first().unwrap().id.clone(),
        candidate: chain.commits.get(1).unwrap().id.clone(),
        chain,
        output,
    };
    let path = minted
        .output
        .path()
        .join("debt.json")
        .to_str()
        .unwrap()
        .to_owned();
    (minted, path)
}

/// The minted snapshot is schema-shaped, reader-clean, and carries the
/// pre-existing blocking finding with the adoption metadata verbatim.
#[test]
fn a_minted_snapshot_clears_its_own_reader_and_schema() {
    let (minted, path) = minted();
    let args = adopt_args(&minted, &path);
    let shown: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, stdout, _stderr) = amiss(&shown);
    assert_eq!(code, 0, "{}", String::from_utf8(stdout).unwrap());
    let bytes = fs::read(&path).unwrap();
    let snapshot = parse_debt_snapshot(&bytes).unwrap();
    assert_eq!(snapshot.items.len(), 1);
    let schema: serde_json::Value = serde_json::from_slice(
        &fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../spec/debt-snapshot.schema.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(
        validator.iter_errors(&document).next().is_none(),
        "the minted file clears the public schema"
    );
    assert_eq!(document["items"][0]["owner"], "team:docs-platform");
    assert_eq!(document["organization_floor_digest"], floor_digest());
}

/// The centerpiece: the same evaluation that failed the finding tolerates it
/// once the minted snapshot rides in as the debt control.
#[test]
#[expect(clippy::too_many_lines, reason = "one end-to-end adoption path")]
fn a_minted_snapshot_round_trips_into_tolerance() {
    let (minted, path) = minted();
    let args = adopt_args(&minted, &path);
    let shown: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, _stdout, _stderr) = amiss(&shown);
    assert_eq!(code, 0);
    let snapshot = parse_debt_snapshot(&fs::read(&path).unwrap()).unwrap();

    let repo = Repository::open(minted.chain.root(), ObjectFormat::Sha1).unwrap();
    let base = Oid::new(ObjectFormat::Sha1, minted.base.clone()).unwrap();
    let candidate = Oid::new(ObjectFormat::Sha1, minted.candidate.clone()).unwrap();
    let engine = amiss_wire::report::EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: amiss_wire::digest::hb("amiss/scanner-engine", b"test engine"),
    };
    let identity =
        amiss_wire::model::RepositoryIdentity::github("acme".to_owned(), "docs".to_owned())
            .unwrap();
    let base_block = SnapshotIdentity {
        commit_oid: base.clone(),
        kind: amiss_wire::requests::GitSnapshotKind::GitCommit,
        object_format: ObjectFormat::Sha1,
        tree_oid: Oid::new(
            ObjectFormat::Sha1,
            minted.chain.commits.first().unwrap().tree.clone(),
        )
        .unwrap(),
    };
    let candidate_block = SnapshotIdentity {
        commit_oid: candidate.clone(),
        kind: amiss_wire::requests::GitSnapshotKind::GitCommit,
        object_format: ObjectFormat::Sha1,
        tree_oid: Oid::new(
            ObjectFormat::Sha1,
            minted.chain.commits.get(1).unwrap().tree.clone(),
        )
        .unwrap(),
    };
    let time_setup = Setup {
        engine: engine.clone(),
        profile: amiss_wire::controls::Profile::Enforce,
        repository: Some(identity.clone()),
        forge: Some(amiss_wire::model::ForgeDialect::Github),
        candidate_ref: BranchRef::new("refs/heads/main".to_owned()),
        target_ref: BranchRef::new("refs/heads/main".to_owned()),
        default_branch_ref: None,
        base: base_block,
        candidate: CandidateBlock::Commit(candidate_block),
        policy: amiss_scan::policy::Effects::default(),
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    };
    let statement = format!(
        r#"{{
  "schema": "amiss/scanner-trusted-time-statement",
  "controller": "external-required-check-clock",
  "repository": {{ "host": "github.com", "owner": "acme", "name": "docs" }},
  "ref": "refs/heads/main",
  "candidate_identity_digest": "{}",
  "provider": "gitlab-ci",
  "provider_run_id": "pipeline/987654321",
  "provider_run_attempt": 2,
  "evaluation_instant": "2026-08-08T12:00:00Z",
  "valid_until": "2026-08-08T12:04:00Z"
}}"#,
        candidate_identity_digest(&time_setup).unwrap()
    );
    let statement = parse_trusted_time(statement.as_bytes()).unwrap();
    let debt_digest = canonical_debt_snapshot(&snapshot).unwrap().1;
    let shell = amiss_scan::pipeline::SetupShell {
        engine,
        profile: amiss_wire::controls::Profile::Enforce,
        repository: Some(identity),
        forge: Some(amiss_wire::model::ForgeDialect::Github),
        candidate_ref: BranchRef::new("refs/heads/main".to_owned()),
        target_ref: BranchRef::new("refs/heads/main".to_owned()),
        default_branch_ref: None,
        floor: Some({
            let floor = parse_organization_floor(FLOOR.as_bytes()).unwrap();
            let digest = canonical_organization_floor(&floor).unwrap().1;
            FloorInput {
                floor,
                digest,
                trust_source: RequestTrust::OrganizationPolicy,
            }
        }),
        debt: Some(DebtInput {
            snapshot,
            digest: debt_digest,
            trust_source: RequestTrust::ExternalRequiredCheck,
        }),
        waiver: None,
        time: Some(TimeInput {
            statement,
            provider: "gitlab-ci".to_owned(),
            provider_run_id: "pipeline/987654321".to_owned(),
            provider_run_attempt: 2,
        }),
        constraint: None,
        semantic: amiss_scan::semantic::Input::None,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let built = commit_pair(&repo, &shell.engine, None, &shell, &base, &candidate);
    let report: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    assert_eq!(
        report["payload"]["summary"]["findings"]["debt_tolerated"], 1,
        "{}",
        report["payload"]["summary"]
    );
    assert_eq!(built.exit_code, 0, "tolerated debt does not block");
}

/// A blocking finding outside the eligible kinds is counted, not recorded:
/// the broken claim stays to be fixed while the snapshot stays empty.
#[test]
fn an_ineligible_blocking_finding_is_counted_not_recorded() {
    let fx = crate::support::claim_fixture();
    let output = TempDir::new().unwrap();
    let path = output.path().join("debt.json").to_str().unwrap().to_owned();
    let args = [
        "adopt",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--repository",
        "github.com/acme/docs",
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
        "--floor-digest",
        &floor_digest(),
        "--debt-owner",
        "team:docs-platform",
        "--debt-reason",
        "adoption",
        "--created-at",
        "2026-08-08T00:00:00Z",
        "--expires-at",
        "2027-08-08T00:00:00Z",
        "--debt-output",
        &path,
    ];
    let (code, stdout, _stderr) = amiss(&args);
    let shown = String::from_utf8(stdout).unwrap();
    assert_eq!(code, 0, "{shown}");
    assert!(shown.contains("0 blocking findings recorded"), "{shown}");
    assert!(
        shown.contains("1 blocking findings are not debt-eligible"),
        "{shown}"
    );
    assert!(shown.contains("0 eligible rows skipped"), "{shown}");
    let snapshot = parse_debt_snapshot(&fs::read(&path).unwrap()).unwrap();
    assert!(snapshot.items.is_empty());
}

/// An existing output path refuses the mint and keeps its bytes.
#[test]
fn an_existing_output_path_refuses_the_mint() {
    let (minted, path) = minted();
    fs::write(&path, b"held").unwrap();
    let args = adopt_args(&minted, &path);
    let shown: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, stdout, _stderr) = amiss(&shown);
    assert_eq!(code, 1, "{}", String::from_utf8(stdout.clone()).unwrap());
    assert!(
        String::from_utf8(stdout)
            .unwrap()
            .contains("already exists")
    );
    assert_eq!(fs::read(&path).unwrap(), b"held");
}

/// The adoption form refuses the staged selector, the profile, and the
/// report output flags, and requires every adoption value.
#[test]
fn the_adopt_form_refuses_foreign_and_missing_flags() {
    let (minted, path) = minted();
    let base = adopt_args(&minted, &path);
    let with = |extra: &[&str]| {
        let mut args = base.clone();
        args.extend(extra.iter().map(|argument| (*argument).to_owned()));
        args
    };
    let dropped = |flag: &str| {
        let mut args = base.clone();
        let at = args.iter().position(|argument| argument == flag).unwrap();
        args.drain(at..=at + 1);
        args
    };
    for args in [
        with(&["--profile", "enforce"]),
        with(&["--explain-scope"]),
        with(&["--index"]),
        {
            let mut args = dropped("--candidate");
            args.push("--index".to_owned());
            args
        },
        dropped("--debt-owner"),
        dropped("--expires-at"),
        dropped("--floor-digest"),
        dropped("--repository"),
    ] {
        let shown: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, _stdout, stderr) = amiss(&shown);
        assert_eq!(code, 2, "{stderr}");
        assert!(stderr.contains("INVALID_INVOCATION"), "{stderr}");
    }
    let args = with(&["--format", "json"]);
    let shown: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, stdout, _stderr) = amiss(&shown);
    assert_eq!(code, 2);
    assert!(
        String::from_utf8(stdout)
            .unwrap()
            .contains("INVALID_INVOCATION"),
        "a machine-format rejection carries the code in its envelope"
    );
}

/// A malformed instant or digest is refused by the grammar, not the writer.
#[test]
fn malformed_adoption_values_are_refused() {
    let (minted, path) = minted();
    let broken = |flag: &str, value: &str| {
        let mut args = adopt_args(&minted, &path);
        let at = args.iter().position(|argument| argument == flag).unwrap();
        args[at + 1] = value.to_owned();
        args
    };
    let uppercase = format!("sha256:{}", "A".repeat(64));
    for args in [
        broken("--created-at", "yesterday"),
        broken("--expires-at", "2027-13-40T99:00:00Z"),
        broken("--floor-digest", "sha256:short"),
        broken("--floor-digest", &uppercase),
        broken("--debt-owner", ""),
        broken("--expires-at", "2026-08-08T00:00:00Z"),
    ] {
        let shown: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, _stdout, stderr) = amiss(&shown);
        assert_eq!(code, 2, "{stderr}");
        assert!(stderr.contains("INVALID_INVOCATION"), "{stderr}");
    }
}
