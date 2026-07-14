#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;

use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair, staged_index};
use amiss_scan::report::RequestDigests;
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

fn bare_shell() -> SetupShell {
    SetupShell {
        engine: engine(),
        enforce: false,
        repository: None,
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

/// The same content every time: a document that resolves, a document that does
/// not, a directory reference, a subdirectory, and a target that moves between
/// the two commits, so the report carries documents, observations, and findings
/// of more than one kind rather than an empty surface that is trivially equal to
/// itself. The staging order is the caller's, and must not matter.
fn content(root: &Path, order: [&str; 3]) {
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    let files: [(&str, &str); 3] = [
        (
            "docs/guide.md",
            "# Guide\n\nSee [the parser](../src/parser.rs) and [the tree](../src/).\n\nAlso [gone](nowhere.md).\n",
        ),
        ("README.md", "# Amiss\n\n[guide](docs/guide.md)\n"),
        ("src/parser.rs", "fn parse() {}\n"),
    ];
    for name in order {
        let (path, body) = files
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .expect("the order names the fixture's own files");
        fs::write(root.join(path), body).unwrap();
    }
    for name in order {
        git(root, &["add", "--", name]);
    }
}

struct Pair {
    _dir: TempDir,
    root: std::path::PathBuf,
    base: Oid,
    candidate: Oid,
}

/// A base and a candidate whose only difference is the body of the file the
/// guide points at, so the candidate carries a real impact finding.
fn pair(order: [&str; 3]) -> Pair {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_owned();
    git(&root, &["init", "-q"]);
    content(&root, order);
    git(&root, &["commit", "-qm", "base"]);

    fs::write(root.join("src/parser.rs"), "fn parse() -> u8 { 7 }\n").unwrap();
    git(&root, &["add", "--", "src/parser.rs"]);
    git(&root, &["commit", "-qm", "candidate"]);

    let base = Oid::new(
        ObjectFormat::Sha1,
        git(&root, &["rev-parse", "HEAD~1"]).trim().to_owned(),
    )
    .unwrap();
    let candidate = Oid::new(
        ObjectFormat::Sha1,
        git(&root, &["rev-parse", "HEAD"]).trim().to_owned(),
    )
    .unwrap();
    Pair {
        _dir: dir,
        root,
        base,
        candidate,
    }
}

fn run(pair: &Pair) -> Vec<u8> {
    let repo = Repository::open(&pair.root, ObjectFormat::Sha1).unwrap();
    commit_pair(
        &repo,
        &engine(),
        None,
        &bare_shell(),
        &pair.base,
        &pair.candidate,
    )
    .wire()
}

/// The report is a function of the repository and the two commits, and of
/// nothing else. Running twice in one process must not move a byte: no clock, no
/// address, no iteration order of a hash map can reach the wire.
#[test]
fn the_same_input_twice_emits_the_same_bytes() {
    let fixture = pair(["README.md", "docs/guide.md", "src/parser.rs"]);
    let first = run(&fixture);
    let second = run(&fixture);
    assert_eq!(first, second, "two runs, one answer");
    assert!(!first.is_empty() && first.ends_with(b"\n"));
}

/// The same objects, reached a different way. Loose objects are found through
/// the two-character fanout; packed objects are found through a pack index and
/// may be deltas against a base that is itself in the pack. That is the widest
/// traversal difference the object store has, and the report may not notice it.
#[test]
fn a_repacked_object_store_emits_the_same_bytes() {
    let fixture = pair(["README.md", "docs/guide.md", "src/parser.rs"]);
    let loose = run(&fixture);
    git(&fixture.root, &["repack", "-adq"]);
    let packed = run(&fixture);
    assert_eq!(
        loose, packed,
        "the same commits, whether their objects are loose or packed"
    );
}

/// Two repositories, the same content, staged in different orders. Git trees are
/// content-addressed and canonically sorted, so this must reach the same trees,
/// the same commits, and byte for byte the same report. What the test really
/// pins is that nothing downstream reintroduces an order: not discovery, not
/// resolution, not the finding sort, not the JSON.
#[test]
fn the_same_content_staged_in_another_order_emits_the_same_bytes() {
    let forward = pair(["README.md", "docs/guide.md", "src/parser.rs"]);
    let reverse = pair(["src/parser.rs", "docs/guide.md", "README.md"]);
    assert_eq!(
        forward.candidate.as_str(),
        reverse.candidate.as_str(),
        "the same content is the same commit"
    );
    assert_eq!(run(&forward), run(&reverse));
}

/// The index-mode and commit-mode reports of the same tree. The spec asks for
/// two things at once, and they pull against each other: every policy-free
/// document, observation, and finding fact must be equal, because the content is
/// the same content, while the snapshot and provenance fields must differ and
/// say which mode produced them, because a staged index is not a commit and a
/// report that blurs the two is lying about what it evaluated.
///
/// This is the fixture that would have caught the commit-versus-index directory
/// disagreement fixed in 2ae3931, which survived until the scanner was run
/// against its own repository by hand.
#[test]
fn index_and_commit_modes_agree_on_every_fact_and_disclose_their_mode() {
    let fixture = pair(["README.md", "docs/guide.md", "src/parser.rs"]);
    let repo = Repository::open(&fixture.root, ObjectFormat::Sha1).unwrap();

    let from_commit = commit_pair(
        &repo,
        &engine(),
        None,
        &bare_shell(),
        &fixture.base,
        &fixture.candidate,
    );
    let from_index = staged_index(&repo, &engine(), None, &bare_shell(), &fixture.base);

    let commit: serde_json::Value = serde_json::from_slice(&from_commit.wire()).unwrap();
    let index: serde_json::Value = serde_json::from_slice(&from_index.wire()).unwrap();
    let (commit, index) = (&commit["payload"], &index["payload"]);

    for fact in ["documents", "observations", "findings", "summary", "result"] {
        assert_eq!(
            commit[fact], index[fact],
            "the index holds the candidate tree, so `{fact}` is the same fact either way"
        );
    }
    assert_eq!(from_commit.exit_code, from_index.exit_code);
    assert!(
        commit["findings"].as_array().unwrap().len() > 1,
        "a fixture that agrees about nothing proves nothing"
    );

    assert_eq!(commit["evaluation"]["mode"], "commit-pair");
    assert_eq!(index["evaluation"]["mode"], "index");
    assert_eq!(commit["evaluation"]["finality"], "explicit-replay");
    assert_eq!(index["evaluation"]["finality"], "local-nonfinal");
    assert_eq!(index["evaluation"]["candidate"]["kind"], "index");
    assert_ne!(
        commit["evaluation"]["candidate"], index["evaluation"]["candidate"],
        "a staged index is not a commit, and the snapshot block says so"
    );
}
