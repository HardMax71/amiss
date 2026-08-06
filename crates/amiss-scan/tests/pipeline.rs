use std::fs;
use std::path::Path;

use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair, staged_index};
use amiss_scan::report::{Built, RequestDigests};
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

fn shell() -> SetupShell {
    SetupShell {
        engine: engine(),
        enforce: false,
        introduced_only: false,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    }
}

#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test assertion helper"
)]
fn payload(built: &Built) -> serde_json::Value {
    let wire: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    wire["payload"].clone()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn base_commit(root: &Path) -> String {
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_owned()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn oid(hex: &str) -> Oid {
    Oid::new(ObjectFormat::Sha1, hex.to_owned()).unwrap()
}

/// A tree path the report cannot spell is disclosed by its raw bytes, which
/// are the only name that entry has.
#[test]
fn an_unrepresentable_tree_path_is_disclosed_by_its_bytes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# X\n").unwrap();
    let raw = br"back\slash.md".as_slice();
    let tree = amiss_fixtures::tree_object(
        root,
        &[
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("100644", raw, blob.as_str()),
        ],
    )
    .unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "unspellable").unwrap();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    );
    let payload = payload(&built);
    let row = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["code"] == "UNREPRESENTABLE_PATH")
        .expect("the walk reports the entry it could not name");
    assert!(row["path"].is_null(), "there is no spelling to print");
    assert_eq!(
        row["path_bytes_hex"].as_str(),
        Some(hex(raw).as_str()),
        "the bytes are the disclosure: {row}"
    );
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        let _infallible = std::fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
    }
    text
}

/// The index discloses an unnameable entry up to the raw-path ceiling and not
/// one byte past it.
#[test]
fn an_index_path_is_disclosed_up_to_the_ceiling() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    let blob = amiss_fixtures::loose_object(root, "blob", b"# X\n").unwrap();
    let mut at_ceiling = vec![b'a'; 4095];
    at_ceiling.push(b'\\');
    let mut past_ceiling = vec![b'b'; 4096];
    past_ceiling.push(b'\\');
    amiss_fixtures::index_file(
        root,
        &[
            (at_ceiling.as_slice(), blob.as_str()),
            (past_ceiling.as_slice(), blob.as_str()),
        ],
    )
    .unwrap();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base));
    let payload = payload(&built);
    let disclosed: Vec<Option<&str>> = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["code"] == "UNREPRESENTABLE_PATH")
        .map(|row| row["path_bytes_hex"].as_str())
        .collect();
    assert_eq!(disclosed.len(), 2, "both entries are refused");
    assert!(
        disclosed.contains(&Some(hex(&at_ceiling).as_str())),
        "the entry at the ceiling is disclosed whole"
    );
    assert!(
        disclosed.contains(&None),
        "the entry past the ceiling has no disclosure to make"
    );
}

/// An index the run cannot read names why the candidate is unavailable.
#[test]
fn an_unreadable_index_names_the_reason_it_left() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join(".git/index"), b"not an index at all").unwrap();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base));
    let payload = payload(&built);
    assert_eq!(payload["evaluation"]["candidate"]["kind"], "unavailable");
    assert_eq!(
        payload["evaluation"]["candidate"]["reasons"],
        serde_json::json!(["index-invalid"])
    );
}

/// The staged run reads its policy from both sides, so a raise staged with
/// the work it governs takes effect on the run that stages it.
#[test]
fn a_staged_policy_raises_the_disposition_of_the_run_that_stages_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join("guide.md"), "see [gone](missing.md)\n").unwrap();
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        br#"{"schema":"amiss/scanner-policy","document_includes":[],"protected_inventory":[],"finding_dispositions":[{"finding_kind":"explicit-target-missing","disposition":"fail"}]}"#,
    )
    .unwrap();
    git(root, &["add", "."]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base));
    let payload = payload(&built);
    let raised: Vec<&serde_json::Value> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|finding| finding["kind"] == "explicit-target-missing")
        .collect();
    assert_eq!(raised.len(), 1, "the staged document has one bad link");
    assert_eq!(
        raised[0]["effective_disposition"], "fail",
        "the staged policy is the one the run answers to: {}",
        raised[0]
    );
}

/// A well-formed value claim whose target agrees is attested: no finding,
/// no boundary, and the summary counts it.
#[test]
fn an_attested_value_claim_passes_and_is_counted() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# R\"\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "claimed"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    );
    let payload = payload(&built);
    assert_eq!(built.exit_code, 0, "{payload}");
    assert_eq!(payload["result"]["status"], "pass");
    assert_eq!(payload["summary"]["governed_claims"], 1);
    assert_eq!(payload["summary"]["unattested_claims"], 0);
    assert_eq!(payload["errors"].as_array().map(Vec::len), Some(0));
    assert!(
        payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "claim-broken"
                && row["kind"] != "claim-target-missing"
                && row["kind"] != "unsupported-capability"),
        "an attested claim leaves no claim finding behind: {}",
        payload["findings"]
    );
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn claimed_run(claim_line: &str, enforce: bool) -> (i64, serde_json::Value) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join("docs.md"), format!("# Docs\n\n{claim_line}\n")).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "claimed"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut setup = shell();
    setup.enforce = enforce;
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &setup,
        &oid(&base),
        &oid(&candidate),
    );
    let code = built.exit_code;
    (code, payload(&built))
}

/// A broken claim warns under observe and fails under enforce, carrying the
/// claim evidence family with both digests.
#[test]
fn a_broken_claim_warns_then_fails_by_profile() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"";
    let (code, payload) = claimed_run(claim, false);
    assert_eq!(code, 0, "observe never blocks: {payload}");
    let row = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "claim-broken")
        .expect("the broken claim is found");
    assert_eq!(row["effective_disposition"], "warn");
    assert_eq!(row["attribution"], "not-applicable");
    assert_eq!(payload["summary"]["governed_claims"], 1);
    assert_eq!(payload["summary"]["unattested_claims"], 1);
    let evidence = &row["candidate_fact"]["evidence"];
    assert_eq!(evidence["kind"], "claim");
    assert_eq!(evidence["claim_kind"], "value");
    assert_eq!(evidence["name"], "v");
    assert_eq!(evidence["target_path"], "README.md");
    assert_eq!(evidence["line"], 1);
    assert_eq!(evidence["observed"], "line-differs");
    assert!(evidence["observed_digest"].is_string());
    assert_eq!(
        row["key_input"]["scope"]["rule_id"], "claim/value/v",
        "the claim name heads the rule id"
    );

    let (code, payload) = claimed_run(claim, true);
    assert_eq!(code, 1, "enforce blocks on a broken claim: {payload}");
}

/// A claim over a target nothing can answer is its own kind, and the reason
/// is named.
#[test]
fn a_claim_target_nothing_answers_is_missing_by_name() {
    let claim = "[amiss:v]: <amiss:value?path=absent.txt&line=L1> \"x\"";
    let (code, payload) = claimed_run(claim, false);
    assert_eq!(code, 0, "{payload}");
    let row = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "claim-target-missing")
        .expect("the missing target is found");
    assert_eq!(
        row["candidate_fact"]["evidence"]["observed"],
        "target-absent"
    );
    assert!(row["candidate_fact"]["evidence"]["observed_digest"].is_null());
    assert_eq!(payload["summary"]["unattested_claims"], 1);
}

/// A document holding one lawful claim and one unknown capability still ends
/// at the boundary, with the claim finding standing beside the refusal.
#[test]
fn an_unknown_capability_beside_a_claim_keeps_the_boundary() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n[amiss:future]: <amiss:region>";
    let (code, payload) = claimed_run(claim, false);
    assert_eq!(code, 2, "the unknown kind is still a boundary: {payload}");
    assert_eq!(payload["result"]["status"], "incomplete");
    let kinds: Vec<&str> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"unsupported-capability"), "{kinds:?}");
    assert!(kinds.contains(&"claim-broken"), "{kinds:?}");
    let capability = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "unsupported-capability")
        .expect("the unknown form");
    assert_eq!(
        capability["aggregation"]["member_count"], 1,
        "only the unanswered node seeds the boundary"
    );
}

/// Two claims under one name aggregate into one finding with both sources.
#[test]
fn duplicate_claim_names_aggregate() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wronger\"";
    let (_code, payload) = claimed_run(claim, false);
    let rows: Vec<&serde_json::Value> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "claim-broken")
        .collect();
    assert_eq!(rows.len(), 1, "one finding for the shared name: {payload}");
    assert_eq!(rows[0]["aggregation"]["member_count"], 2);
    assert_eq!(payload["summary"]["governed_claims"], 2);
    assert_eq!(payload["summary"]["unattested_claims"], 2);
    assert_eq!(
        rows[0]["candidate_fact"]["evidence"]["sources"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "two distinct sources under one name"
    );
}

/// The staged path attests the same claim the commit path does.
#[test]
fn a_staged_claim_attests_like_a_committed_one() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# R\"\n",
    )
    .unwrap();
    git(root, &["add", "."]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base));
    let payload = payload(&built);
    assert_eq!(built.exit_code, 0, "{payload}");
    assert_eq!(payload["summary"]["governed_claims"], 1);
    assert_eq!(payload["summary"]["unattested_claims"], 0);
}

/// Duplicates under one name aggregate per outcome kind: a broken member and
/// an unanswered member are two findings, not one blurred verdict.
#[test]
fn mixed_verdicts_under_one_name_split_by_kind() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n[amiss:v]: <amiss:value?path=absent.txt&line=L1> \"x\"";
    let (_code, payload) = claimed_run(claim, false);
    let kinds: Vec<&str> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["kind"].as_str())
        .filter(|kind| kind.starts_with("claim-"))
        .collect();
    assert_eq!(kinds, ["claim-broken", "claim-target-missing"], "{payload}");
    assert_eq!(payload["summary"]["governed_claims"], 2);
    assert_eq!(payload["summary"]["unattested_claims"], 2);
}
