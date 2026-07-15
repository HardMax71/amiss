use std::fs;
use std::path::Path;

use amiss_fixtures::stage_symlink;
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::{DocumentStatus, Error, ScanLimits, ScanResources, UnsupportedKind, discover};
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

fn run(
    dir: &Path,
    scan_limits: ScanLimits,
    git_limits: GitLimits,
) -> Result<amiss_scan::SnapshotDiscovery, Error> {
    let repo = Repository::open(dir, ObjectFormat::Sha1).map_err(Error::from)?;
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
                record.classification.as_str(),
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
}
