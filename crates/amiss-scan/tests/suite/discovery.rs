use std::fs;
use std::path::Path;
use std::sync::Arc;

use amiss_fixtures::stage_symlink;
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::{
    DocumentStatus, Error, ScanLimits, ScanResources, UnsupportedKind, discover, discover_index,
};
use amiss_wire::controls::ResourceName;
use amiss_wire::model::{ObjectFormat, Oid};
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn head_tree(dir: &Path) -> Oid {
    let hex = git(dir, &["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    Oid::new(ObjectFormat::Sha1, hex).unwrap()
}

const POINTER: &str = "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n";

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n\n[home](../README)\n").unwrap();
    fs::write(root.join("docs/page.mdx"), "{1 + 1}\n").unwrap();
    fs::write(root.join("notes.txt"), "not a document\n").unwrap();
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("vendor/skip.md"), "[v](x)\n").unwrap();
    fs::write(root.join("llms.txt"), "plain advisory body\n").unwrap();
    fs::write(root.join("pointer.md"), POINTER).unwrap();
    git(root, &["add", "."]);
    stage_symlink(root, "README", "linked.md").unwrap();
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,0123456789012345678901234567890123456789,CHANGELOG",
        ],
    );
    git(root, &["commit", "-qm", "fixture"]);
    dir
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn run(
    dir: &Path,
    scan_limits: ScanLimits,
    git_limits: GitLimits,
) -> Result<amiss_scan::SnapshotDiscovery, Error> {
    let repo = Repository::open(dir, ObjectFormat::Sha1).expect("fixture repository opens");
    let mut git_resources = GitResources::new(git_limits);
    let mut scan_resources = ScanResources::new(scan_limits);
    discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &amiss_scan::Includes::default(),
        &head_tree(dir),
    )
}

#[test]
fn a_snapshot_discovers_every_class_in_path_order() {
    let dir = fixture();
    let got = run(dir.path(), ScanLimits::CONTRACT, GitLimits::CONTRACT).unwrap();

    let listing: Vec<(String, &'static str, &'static str)> = got
        .documents
        .iter()
        .map(|record| {
            let status = match &record.status {
                DocumentStatus::Scanned(_) => "scanned",
                DocumentStatus::ExcludedBuiltIn => "excluded",
                DocumentStatus::Unsupported(UnsupportedKind::Symlink) => "symlink",
                DocumentStatus::Unsupported(UnsupportedKind::Gitlink) => "gitlink",
                DocumentStatus::Unsupported(UnsupportedKind::LfsPointer) => "lfs-pointer",
                DocumentStatus::Unsupported(UnsupportedKind::Format) => "unsupported-format",
                DocumentStatus::Failed(_) => "failed",
            };
            (
                record.path.as_str().unwrap().to_owned(),
                record.classification.into(),
                status,
            )
        })
        .collect();
    assert_eq!(
        listing,
        vec![
            ("CHANGELOG".to_owned(), "extensionless-markdown", "gitlink"),
            ("README".to_owned(), "extensionless-markdown", "scanned"),
            ("docs/guide.md".to_owned(), "structured-markdown", "scanned"),
            ("docs/page.mdx".to_owned(), "structured-mdx", "scanned"),
            ("linked.md".to_owned(), "structured-markdown", "symlink"),
            ("llms.txt".to_owned(), "plain-advisory", "scanned"),
            (
                "pointer.md".to_owned(),
                "structured-markdown",
                "lfs-pointer"
            ),
            (
                "vendor/skip.md".to_owned(),
                "structured-markdown",
                "excluded"
            ),
        ]
    );
    assert_eq!(got.outside_document_set, 1, "notes.txt alone");
    assert_eq!(got.path_defects, Vec::new());
    assert_eq!(
        got.tree_entries, 11,
        "nine root entries plus two under docs/"
    );

    let readme = got
        .documents
        .iter()
        .find(|record| record.path.as_bytes() == b"README")
        .unwrap();
    let DocumentStatus::Scanned(scanned) = &readme.status else {
        panic!("README scans")
    };
    assert_eq!(scanned.occurrences.len(), 1);
    assert_eq!(
        scanned
            .occurrences
            .first()
            .map(|entry| entry.occurrence.raw_destination.clone()),
        Some("docs/guide.md".to_owned())
    );

    let mdx = got
        .documents
        .iter()
        .find(|record| record.path.as_bytes() == b"docs/page.mdx")
        .unwrap();
    let DocumentStatus::Scanned(scanned) = &mdx.status else {
        panic!("the mdx page scans")
    };
    assert_eq!(scanned.opaque.mdx.len(), 1, "the expression is opaque");
}

#[test]
fn tree_and_index_discovery_preserve_strict_raw_path_order() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("a-.md"), "first\n").unwrap();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/inside.md"), "middle\n").unwrap();
    fs::write(root.join("a0.md"), "last\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "ordered"]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let includes = amiss_scan::Includes::default();
    let from_tree = discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &head_tree(root),
    )
    .unwrap();
    let index_bytes = repo.read_index_bytes(&mut git_resources).unwrap();
    let index = amiss_git::parse_index_file(ObjectFormat::Sha1, &index_bytes).unwrap();
    let from_index = discover_index(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &index,
    )
    .unwrap();

    let paths = |discovery: &amiss_scan::SnapshotDiscovery| {
        discovery
            .documents
            .iter()
            .map(|record| record.path.as_bytes().to_vec())
            .collect::<Vec<_>>()
    };
    let tree_paths = paths(&from_tree);
    let index_paths = paths(&from_index);
    assert_eq!(tree_paths, index_paths);
    assert_eq!(
        tree_paths,
        [
            b"a-.md".to_vec(),
            b"a/inside.md".to_vec(),
            b"a0.md".to_vec()
        ]
    );
    assert!(
        tree_paths
            .windows(2)
            .all(|pair| matches!(pair, [left, right] if left < right)),
        "document paths are strictly increasing and therefore unique"
    );
}

#[test]
fn excluded_documents_are_never_admitted_or_read() {
    let dir = fixture();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let got = discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &amiss_scan::Includes::default(),
        &head_tree(dir.path()),
    )
    .unwrap();
    let scanned = got
        .documents
        .iter()
        .filter(|record| matches!(record.status, DocumentStatus::Scanned(_)))
        .count();
    assert_eq!(
        scan_resources.documents(),
        u64::try_from(scanned).unwrap().saturating_add(1),
        "admissions are the scanned documents plus the pointer; excluded and \
         symlink and gitlink sides are never admitted"
    );
}

#[test]
fn snapshot_budgets_end_discovery() {
    let dir = fixture();

    let tight_documents = ScanLimits {
        documents_per_snapshot: 2,
        ..ScanLimits::CONTRACT
    };
    assert_eq!(
        run(dir.path(), tight_documents, GitLimits::CONTRACT),
        Err(Error::ResourceLimit {
            resource: ResourceName::DocumentsPerSnapshot,
            configured_limit: 2,
            observed_lower_bound: 3,
        })
    );

    let tight_entries = GitLimits {
        tree_entries_per_snapshot: 4,
        ..GitLimits::CONTRACT
    };
    assert_eq!(
        run(dir.path(), ScanLimits::CONTRACT, tight_entries),
        Err(Error::ResourceLimit {
            resource: ResourceName::GitTreeEntriesPerSnapshot,
            configured_limit: 4,
            observed_lower_bound: 5,
        })
    );

    let tight_aggregate = ScanLimits {
        aggregate_document_bytes_per_snapshot: 40,
        ..ScanLimits::CONTRACT
    };
    let got = run(dir.path(), tight_aggregate, GitLimits::CONTRACT);
    let Err(Error::ResourceLimit {
        resource: ResourceName::AggregateDocumentBytesPerSnapshot,
        configured_limit: 40,
        observed_lower_bound,
    }) = got
    else {
        panic!("expected the aggregate crossing, got {got:?}")
    };
    assert!(observed_lower_bound > 40);
}

#[test]
fn an_oversized_document_fails_alone() {
    let dir = fixture();
    let tight = ScanLimits {
        document_blob_bytes: 24,
        ..ScanLimits::CONTRACT
    };
    let got = run(dir.path(), tight, GitLimits::CONTRACT).unwrap();
    let readme = got
        .documents
        .iter()
        .find(|record| record.path.as_bytes() == b"README")
        .unwrap();
    assert_eq!(
        readme.status,
        DocumentStatus::Failed(Error::ResourceLimit {
            resource: ResourceName::DocumentBlobBytes,
            configured_limit: 24,
            observed_lower_bound: 32,
        }),
        "the header-declared size is observed exactly and only this document fails"
    );
    assert!(
        got.documents
            .iter()
            .any(|record| record.path.as_bytes() == b"docs/page.mdx"
                && matches!(record.status, DocumentStatus::Scanned(_))),
        "smaller documents after the oversized one still scan"
    );
}

#[test]
fn a_shared_subtree_expands_at_every_path() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    for parent in ["a", "b"] {
        fs::create_dir_all(root.join(parent).join("dup")).unwrap();
        fs::write(root.join(parent).join("dup/x.md"), "[shared](y)\n").unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "dag"]);
    let a_tree = git(root, &["rev-parse", "HEAD:a"]).trim().to_owned();
    let b_tree = git(root, &["rev-parse", "HEAD:b"]).trim().to_owned();
    assert_eq!(a_tree, b_tree, "identical subtrees share one OID");

    let got = run(root, ScanLimits::CONTRACT, GitLimits::CONTRACT).unwrap();
    let paths: Vec<String> = got
        .documents
        .iter()
        .map(|record| record.path.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        paths,
        vec!["a/dup/x.md".to_owned(), "b/dup/x.md".to_owned()]
    );
    assert_eq!(
        got.tree_entries, 6,
        "two roots, two shared dup trees, two blobs: the shared subtree charges at each path"
    );

    let [first, second] = got.documents.as_slice() else {
        panic!("the shared subtree yields two documents")
    };
    let (DocumentStatus::Scanned(first), DocumentStatus::Scanned(second)) =
        (&first.status, &second.status)
    else {
        panic!("both shared documents scan")
    };
    assert!(
        Arc::ptr_eq(first, second),
        "one content-addressed scan backs both paths"
    );

    let tight_nodes = ScanLimits {
        parser_nodes_per_snapshot: first.work.nodes,
        ..ScanLimits::CONTRACT
    };
    assert_eq!(
        run(root, tight_nodes, GitLimits::CONTRACT),
        Err(Error::ResourceLimit {
            resource: ResourceName::ParserNodesPerSnapshot,
            configured_limit: first.work.nodes,
            observed_lower_bound: first.work.nodes.saturating_add(1),
        }),
        "a reused scan still charges its node work"
    );

    let tight_references = ScanLimits {
        references_per_snapshot: 1,
        ..ScanLimits::CONTRACT
    };
    assert_eq!(
        run(root, tight_references, GitLimits::CONTRACT),
        Err(Error::ResourceLimit {
            resource: ResourceName::ReferencesPerSnapshot,
            configured_limit: 1,
            observed_lower_bound: 2,
        }),
        "a reused scan still charges its extracted references"
    );
}

#[test]
fn mdx_reuse_requires_the_same_embedded_code_allowance() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    let source = "a {'}'} b\n";
    fs::write(root.join("a.mdx"), source).unwrap();
    fs::write(root.join("b.mdx"), source).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "mdx"]);

    let got = run(root, ScanLimits::CONTRACT, GitLimits::CONTRACT).unwrap();
    let [first, second] = got.documents.as_slice() else {
        panic!("the fixture yields two MDX documents")
    };
    let (DocumentStatus::Scanned(first), DocumentStatus::Scanned(second)) =
        (&first.status, &second.status)
    else {
        panic!("both MDX documents scan")
    };
    assert!(first.embedded_code_bytes > 0, "the source spends allowance");
    assert!(
        !Arc::ptr_eq(first, second),
        "the first scan changes the allowance before the second"
    );
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn label_fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(
        root.join("a.rst"),
        ".. _`Wide  Name`:\n\n.. _shared:\n\nA\n=\n",
    )
    .unwrap();
    fs::write(root.join("b.rst"), ".. _shared:\n\nB\n=\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "labels"]);
    dir
}

/// The table stores the Docutils simple name, so a quoted wide declaration
/// answers a folded lookup, and a name two documents claim is marked rather
/// than guessed between.
#[test]
fn the_label_table_normalizes_and_marks_duplicates() {
    let dir = label_fixture();
    let got = run(dir.path(), ScanLimits::CONTRACT, GitLimits::CONTRACT).unwrap();
    assert_eq!(
        got.labels.get("wide name"),
        Some(&amiss_scan::LabelState::Declared(
            amiss_wire::model::RepoPath::new("a.rst".to_owned()).unwrap()
        )),
    );
    assert_eq!(
        got.labels.get("shared"),
        Some(&amiss_scan::LabelState::Duplicated),
    );
    assert_eq!(got.labels.len(), 2, "{:?}", got.labels);
}

/// The third declared label crosses a two-label ceiling, and the crossing
/// carries the resource name and both figures.
#[test]
fn the_label_ceiling_ends_discovery_one_past_the_limit() {
    let dir = label_fixture();
    let tight = ScanLimits {
        declared_labels_per_snapshot: 2,
        ..ScanLimits::CONTRACT
    };
    assert_eq!(
        run(dir.path(), tight, GitLimits::CONTRACT),
        Err(Error::ResourceLimit {
            resource: ResourceName::DeclaredLabelsPerSnapshot,
            configured_limit: 2,
            observed_lower_bound: 3,
        })
    );

    let exact = ScanLimits {
        declared_labels_per_snapshot: 3,
        ..ScanLimits::CONTRACT
    };
    assert!(
        run(dir.path(), exact, GitLimits::CONTRACT).is_ok(),
        "a snapshot sitting exactly on the ceiling passes"
    );
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn index_of(dir: &Path) -> (Repository, amiss_git::LogicalIndex) {
    let repo = Repository::open(dir, ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let bytes = repo.read_index_bytes(&mut git_resources).unwrap();
    let index = amiss_git::parse_index_file(ObjectFormat::Sha1, &bytes).unwrap();
    (repo, index)
}

fn discovered_paths(discovery: &amiss_scan::SnapshotDiscovery) -> Vec<String> {
    discovery
        .documents
        .iter()
        .filter_map(|record| record.path.as_str().map(str::to_owned))
        .collect()
}

/// Every snapshot ceiling admits its own value, from either side of the walk.
#[test]
fn discovery_admits_a_snapshot_exactly_at_its_ceilings() {
    let dir = fixture();
    let entries = match run(dir.path(), ScanLimits::CONTRACT, GitLimits::CONTRACT) {
        Ok(discovery) => discovery.tree_entries,
        Err(defect) => panic!("the fixture discovers: {defect:?}"),
    };
    assert!(entries > 0, "the fixture holds entries");

    let at_ceiling = GitLimits {
        tree_entries_per_snapshot: entries,
        ..GitLimits::CONTRACT
    };
    assert!(
        run(dir.path(), ScanLimits::CONTRACT, at_ceiling).is_ok(),
        "a tree walk at exactly its entry ceiling"
    );

    let (repo, index) = index_of(dir.path());
    let index_walk = |limits: GitLimits| {
        let mut git_resources = GitResources::new(limits);
        let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
        discover_index(
            &repo,
            &mut git_resources,
            &mut scan_resources,
            &amiss_scan::Includes::default(),
            &index,
        )
    };
    let index_entries = index_walk(GitLimits::CONTRACT)
        .expect("the fixture index discovers")
        .tree_entries;
    let index_ceiling = GitLimits {
        tree_entries_per_snapshot: index_entries,
        ..GitLimits::CONTRACT
    };
    assert!(
        index_walk(index_ceiling).is_ok(),
        "an index walk at exactly its entry ceiling"
    );
    let index_short = GitLimits {
        tree_entries_per_snapshot: index_entries.saturating_sub(1),
        ..GitLimits::CONTRACT
    };
    assert!(
        index_walk(index_short).is_err(),
        "one entry past the index ceiling"
    );

    let longest =
        discovered_paths(&run(dir.path(), ScanLimits::CONTRACT, GitLimits::CONTRACT).unwrap())
            .into_iter()
            .map(|path| u64::try_from(path.len()).unwrap())
            .max()
            .expect("the fixture holds documents");
    let at_path_ceiling = GitLimits {
        raw_path_bytes: longest,
        ..GitLimits::CONTRACT
    };
    let discovery = run(dir.path(), ScanLimits::CONTRACT, at_path_ceiling).unwrap();
    assert!(
        discovery.path_defects.is_empty(),
        "a path exactly at its byte ceiling is admitted"
    );
}

/// A path the classifier does not know is admitted when policy names it, on
/// both walks, and a gitlink is never asked for its object.
#[test]
fn policy_includes_and_gitlinks_survive_both_walks() {
    let dir = fixture();
    let includes = amiss_scan::Includes {
        documents: [amiss_wire::model::RepoPath::new("notes.txt".to_owned()).unwrap()]
            .into_iter()
            .collect(),
        trees: std::collections::BTreeSet::new(),
        ..amiss_scan::Includes::default()
    };

    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let from_tree = discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &head_tree(dir.path()),
    )
    .unwrap();
    assert!(
        discovered_paths(&from_tree).contains(&"notes.txt".to_owned()),
        "policy admits what the classifier does not know"
    );

    let (repo, index) = index_of(dir.path());
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let from_index = discover_index(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &index,
    )
    .expect("the index walk holds a gitlink whose object nobody has");
    assert!(discovered_paths(&from_index).contains(&"notes.txt".to_owned()));
}

/// A defect the document owns fails that document; a defect the snapshot owns
/// ends the run.
#[test]
fn a_snapshot_scoped_defect_ends_the_walk() {
    let dir = fixture();
    let tight = ScanLimits {
        parser_nodes_per_snapshot: 1,
        ..ScanLimits::CONTRACT
    };
    let got = run(dir.path(), tight, GitLimits::CONTRACT);
    assert!(
        matches!(
            got,
            Err(Error::ResourceLimit {
                resource: ResourceName::ParserNodesPerSnapshot,
                ..
            })
        ),
        "a snapshot budget is nobody's document: {got:?}"
    );
}
