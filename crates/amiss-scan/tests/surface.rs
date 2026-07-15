#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;

use amiss_fixtures::stage_symlink;
use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::{Built, RequestDigests};
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine/v1", b"test engine"),
    }
}

/// The shell a real invocation builds. The frozen grammar has no control-supply
/// surface, so `amiss check` leaves every control absent, and an empty surface
/// has to be honest without one.
fn bare_shell() -> SetupShell {
    SetupShell {
        engine: engine(),
        enforce: false,
        repository: None,
        forge: None,
        candidate_ref: None,
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

const POINTER: &str = "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n";

/// Scans one staged candidate against a base that holds no documents, so every
/// count in the report is the candidate's own surface and nothing carried in
/// from the other side. The closure stages the candidate itself, because a
/// symlink or a gitlink entry is recorded through the index rather than written
/// into the worktree.
fn scan(stage: impl FnOnce(&Path)) -> (Built, serde_json::Value) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join(".gitkeep"), "").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);

    stage(root);
    git(root, &["commit", "-qm", "candidate"]);

    let base = Oid::new(
        ObjectFormat::Sha1,
        git(root, &["rev-parse", "HEAD~1"]).trim().to_owned(),
    )
    .unwrap();
    let candidate = Oid::new(
        ObjectFormat::Sha1,
        git(root, &["rev-parse", "HEAD"]).trim().to_owned(),
    )
    .unwrap();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(&repo, &engine(), None, &bare_shell(), &base, &candidate);
    let wire: serde_json::Value = serde_json::from_slice(&built.wire()).unwrap();
    let payload = wire["payload"].clone();
    (built, payload)
}

fn kinds(payload: &serde_json::Value) -> Vec<String> {
    payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["kind"].as_str().unwrap().to_owned())
        .collect()
}

fn count(payload: &serde_json::Value, group: &str, key: &str) -> u64 {
    payload["summary"][group][key]
        .as_u64()
        .expect("every summary counter is a number")
}

/// Whatever else it says, a report that skipped work must never claim its
/// counts are whole. Every fixture here holds this.
fn complete(built: &Built, payload: &serde_json::Value) {
    assert_eq!(
        payload["summary"]["counts_complete"], true,
        "the counts are whole"
    );
    assert_eq!(payload["result"]["complete"], true, "the run finished");
    assert_eq!(payload["errors"].as_array().unwrap().len(), 0);
    assert_eq!(built.exit_code, 0, "an empty surface is not a failure");
}

/// A repository with nothing to check still owes a report, and every
/// denominator in it is zero and says so. The pass is earned by there being no
/// work, which is a different claim from work that went unreported, and the
/// report has to be able to tell those apart.
#[test]
fn a_repository_with_no_documents_reports_an_empty_surface() {
    let (built, payload) = scan(|root| {
        fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        git(root, &["add", "."]);
    });
    complete(&built, &payload);

    assert_eq!(count(&payload, "documents", "discovered"), 0);
    assert_eq!(count(&payload, "documents", "scanned"), 0);
    assert_eq!(count(&payload, "documents", "unsupported"), 0);
    assert_eq!(count(&payload, "documents", "excluded_builtin"), 0);
    assert_eq!(count(&payload, "documents", "unlinked"), 0);
    assert_eq!(count(&payload, "references", "extracted"), 0);
    assert_eq!(count(&payload, "findings", "total"), 0);

    assert_eq!(payload["documents"].as_array().unwrap().len(), 0);
    assert_eq!(payload["observations"].as_array().unwrap().len(), 0);
    assert_eq!(payload["findings"].as_array().unwrap().len(), 0);
    assert_eq!(payload["result"]["status"], "pass");
}

/// A document that references nothing is scanned, not skipped, and the report
/// names it. The spec binds three numbers together for exactly this case: the
/// summary's `documents.unlinked`, the count of matching candidate documents,
/// and the count of `unlinked-document` findings are one value, so a document
/// cannot go unreferenced and unmentioned at the same time.
#[test]
fn a_document_with_no_references_is_unlinked_exactly_once() {
    let (built, payload) = scan(|root| {
        fs::write(root.join("README.md"), "# Title\n\nProse, and no links.\n").unwrap();
        git(root, &["add", "."]);
    });
    complete(&built, &payload);

    assert_eq!(count(&payload, "documents", "discovered"), 1);
    assert_eq!(count(&payload, "documents", "scanned"), 1);
    assert_eq!(count(&payload, "references", "extracted"), 0);
    assert_eq!(payload["observations"].as_array().unwrap().len(), 0);

    let emitted = kinds(&payload)
        .iter()
        .filter(|kind| kind.as_str() == "unlinked-document")
        .count();
    assert_eq!(emitted, 1, "one finding for the one unlinked document");
    assert_eq!(
        u64::try_from(emitted).unwrap(),
        count(&payload, "documents", "unlinked"),
        "the summary count and the findings are the same number"
    );
    assert_eq!(payload["result"]["status"], "pass");
}

/// Every document is one the scanner cannot read: a symlink, a gitlink, and an
/// LFS pointer. None of them is scanned and none of them vanishes. The unlinked
/// law is the sharp edge here, because an unsupported document has zero
/// extracted references without ever having been read, and calling it unlinked
/// would be a coverage claim the scanner did not earn.
#[test]
fn documents_it_cannot_read_are_disclosed_and_never_counted_as_covered() {
    let (built, payload) = scan(|root| {
        fs::write(root.join("pointer.md"), POINTER).unwrap();
        fs::write(root.join("real.txt"), "the symlink target\n").unwrap();
        git(root, &["add", "."]);
        stage_symlink(root, "real.txt", "linked.md").unwrap();
        git(
            root,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,0123456789012345678901234567890123456789,CHANGELOG",
            ],
        );
    });
    complete(&built, &payload);

    assert_eq!(count(&payload, "documents", "discovered"), 3);
    assert_eq!(count(&payload, "documents", "scanned"), 0);
    assert_eq!(count(&payload, "documents", "unsupported"), 3);
    assert_eq!(count(&payload, "references", "extracted"), 0);

    assert_eq!(
        count(&payload, "documents", "unlinked"),
        0,
        "a document nobody read is not a document with no references"
    );
    assert!(
        !kinds(&payload).contains(&"unlinked-document".to_owned()),
        "no unlinked finding for an unsupported document"
    );

    let rows = payload["documents"].as_array().unwrap();
    let mut named: Vec<&str> = rows
        .iter()
        .map(|row| row["path"].as_str().unwrap())
        .collect();
    named.sort_unstable();
    assert_eq!(
        named,
        vec!["CHANGELOG", "linked.md", "pointer.md"],
        "each one is a row, not a silence"
    );
}

/// An MDX document whose whole body is opaque: an import and a component, and
/// no Markdown the parser can see into. It is scanned, so it is unlinked, and
/// its opacity is a finding rather than an absence. The two claims have to
/// stand together, because "no references here" and "this region is beyond me"
/// are the coverage answer only when both are said.
#[test]
fn an_opaque_only_mdx_document_reports_both_its_silence_and_its_opacity() {
    let (built, payload) = scan(|root| {
        fs::write(
            root.join("page.mdx"),
            "import {Note} from \"./note.js\";\n\n<Note>{\"see the docs\"}</Note>\n",
        )
        .unwrap();
        git(root, &["add", "."]);
    });
    complete(&built, &payload);

    assert_eq!(count(&payload, "documents", "discovered"), 1);
    assert_eq!(count(&payload, "documents", "scanned"), 1);
    assert_eq!(count(&payload, "references", "extracted"), 0);
    assert_eq!(count(&payload, "documents", "unlinked"), 1);
    assert_eq!(count(&payload, "documents", "opaque_mdx_documents"), 1);
    assert!(
        count(&payload, "documents", "opaque_mdx_regions") > 0,
        "the regions it could not see into are counted"
    );

    let emitted = kinds(&payload);
    assert!(
        emitted.contains(&"unlinked-document".to_owned()),
        "it extracted nothing: {emitted:?}"
    );
    assert!(
        emitted.contains(&"opaque-mdx-region".to_owned()),
        "and it says why it extracted nothing: {emitted:?}"
    );
}

/// Raw HTML is opaque, and a report that stays quiet about it is claiming coverage
/// it does not have. A `<div>` can wrap anything, including references this scanner
/// will never see, so the honest answer is a row saying there is a region it could
/// not read into. The MDX half of that promise is tested above. The HTML half emits
/// `opaque-html-region`, and nothing exercised it, which is exactly how a finding
/// kind stops being emitted without anyone noticing.
#[test]
fn a_document_with_raw_html_reports_the_regions_it_cannot_see_into() {
    let (built, payload) = scan(|root| {
        fs::write(root.join("guide.md"), "# Guide\n").unwrap();
        fs::write(
            root.join("page.md"),
            "# Page\n\n<div class=\"card\">\n\n[visible](guide.md)\n\n</div>\n",
        )
        .unwrap();
        git(root, &["add", "."]);
    });
    complete(&built, &payload);

    assert_eq!(
        count(&payload, "documents", "opaque_html_documents"),
        1,
        "the document carrying raw HTML is counted once"
    );
    assert!(
        count(&payload, "documents", "opaque_html_regions") > 0,
        "the regions it could not see into are counted"
    );
    assert!(
        count(&payload, "documents", "opaque_html_bytes") > 0,
        "so are the bytes inside them"
    );

    let emitted = kinds(&payload);
    assert!(
        emitted.contains(&"opaque-html-region".to_owned()),
        "the scan says out loud that it could not read the HTML: {emitted:?}"
    );
}
