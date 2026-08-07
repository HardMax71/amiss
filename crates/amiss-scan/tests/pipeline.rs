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

/// Two same-subject defects fold into one item annotated at the least
/// location, whatever order the fold visits the findings.
#[test]
fn a_grouped_item_annotates_its_least_location() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(root.join("target.md"), "# One\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("README.md"),
        "[b](target.md#also-gone)\n\n[a](target.md#nope)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "links"]);
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
    let items = payload["feedback"]["items"].as_array().unwrap();
    let item = items
        .iter()
        .find(|item| item["action"] == "fix" && item["target"] == "target.md")
        .unwrap_or_else(|| panic!("one grouped fix item: {items:?}"));
    assert_eq!(item["location_count"], 2, "{item}");
    assert_eq!(item["annotation"]["path"], "README.md");
    assert_eq!(
        item["annotation"]["span"]["start_line"], 1,
        "the earliest defect owns the annotation: {item}"
    );
}

/// The fix refuses to guess: an observed line the title grammar cannot
/// spell, a missing target, and a grouped name all emit null.
#[test]
fn a_fix_is_emitted_only_when_provable() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "say \"hi\"\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"other\"\n",
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
    assert_eq!(payload["findings"][0]["kind"], "claim-broken", "{payload}");
    assert!(
        payload["findings"][0]["fix"].is_null(),
        "a quoted observed line cannot be respelled: {payload}"
    );

    let missing = "[amiss:v]: <amiss:value?path=gone.md&line=L1> \"words\"";
    let (_code, payload) = claimed_run(missing, false);
    assert!(
        payload["findings"][0]["fix"].is_null(),
        "a missing target has no derivable content: {payload}"
    );

    let duplicated = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"one\"\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"two\"";
    let (_code, payload) = claimed_run(duplicated, false);
    assert_eq!(payload["findings"][0]["aggregation"]["member_count"], 2);
    assert!(
        payload["findings"][0]["fix"].is_null(),
        "grouped members share one finding but not one edit: {payload}"
    );
}

/// A lone case-drifted anchor carries its fix: the fragment bytes and the
/// published spelling. Aggregated references, bare misses, and spellings the
/// adapter refused to locate all stay bare.
#[test]
fn a_case_drifted_anchor_carries_its_fix() {
    let run = |links: &str| -> serde_json::Value {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        fs::write(root.join("README.md"), "# R\n").unwrap();
        fs::write(root.join("sections.md"), "# Setup\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "base"]);
        let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        fs::write(root.join("guide.md"), links).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "linked"]);
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
        payload(&built)
    };

    let drifted = run("[s](sections.md#Setup)\n");
    let row = drifted["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    let fix = &row["fix"];
    assert_eq!(fix["path"], "guide.md", "{row}");
    assert_eq!(fix["replacement"], "setup");
    let source = "[s](sections.md#Setup)\n";
    let start = usize::try_from(fix["span"]["start_byte"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(fix["span"]["end_byte"].as_u64().unwrap()).unwrap();
    assert_eq!(&source.as_bytes()[start..end], b"Setup");
    assert_eq!(
        fix["description"],
        "replace the fragment with the one published anchor it matches under case folding"
    );

    let same_block = run("[a](sections.md#Setup) and [b](sections.md#Setup)\n");
    let row = same_block["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(row["aggregation"]["member_count"], 2, "{same_block}");
    assert!(row["fix"].is_null(), "two members share no one edit: {row}");

    let two_blocks = run("[a](sections.md#Setup)\n\n[b](sections.md#Setup)\n");
    let fixes: Vec<(u64, u64)> = two_blocks["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "explicit-target-missing")
        .map(|row| {
            (
                row["fix"]["span"]["start_byte"].as_u64().unwrap(),
                row["fix"]["span"]["end_byte"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(fixes.len(), 2, "each block's reference repairs itself");
    assert_ne!(fixes[0], fixes[1], "each fix names its own bytes");

    let bare = run("[s](sections.md#nothing-close)\n");
    let row = bare["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert!(row["fix"].is_null(), "no neighbor, no edit: {row}");

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(root.join("sections.md"), "# Setup\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let rst = "See `s <sections.md#Setup>`_ here.\n";
    fs::write(root.join("guide.rst"), rst).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "linked"]);
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
    let rst_payload = payload(&built);
    let row = rst_payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(
        row["fix"]["replacement"], "setup",
        "the rst lane carries the anchor fix too: {row}"
    );
    let start = usize::try_from(row["fix"]["span"]["start_byte"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(row["fix"]["span"]["end_byte"].as_u64().unwrap()).unwrap();
    assert_eq!(&rst.as_bytes()[start..end], b"Setup");

    let encoded = run("[s](sections.md#S%65tup)\n");
    let row = encoded["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert!(
        row["fix"].is_null(),
        "a percent spelling names no bytes: {row}"
    );
}

/// A claim only the base holds is invisible: evaluation is candidate-side,
/// so a broken claim the candidate deletes leaves no count and no finding.
#[test]
fn a_base_side_claim_is_not_evaluated() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("docs.md"), "# Docs\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "stripped"]);
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
    assert_eq!(payload["summary"]["governed_claims"], 0);
    assert_eq!(payload["summary"]["unattested_claims"], 0);
    assert!(
        payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "claim-broken"
                && row["kind"] != "claim-target-missing"
                && row["kind"] != "unsupported-capability"),
        "the base-side claim leaks nothing: {}",
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
    let fix = &payload["findings"][0]["fix"];
    assert_eq!(fix["path"], "docs.md", "{payload}");
    let rewritten = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# R\"";
    assert_eq!(fix["replacement"], rewritten);
    assert_eq!(
        fix["span"]["start_byte"], 8,
        "the definition follows the heading"
    );
    assert_eq!(
        fix["span"]["end_byte"].as_u64().map(|end| end - 8),
        Some(u64::try_from(claim.len()).unwrap()),
        "the span covers the whole definition: {fix}"
    );
    assert_eq!(
        fix["description"],
        "replace the definition so the claim expects the target's current line"
    );
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
