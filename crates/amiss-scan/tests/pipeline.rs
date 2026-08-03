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
