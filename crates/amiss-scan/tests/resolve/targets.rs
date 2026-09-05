use std::fs;

use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::resolve::{RAW_EVIDENCE_DOMAIN, Resolver, TARGET_PROJECTION_DOMAIN, TargetCache};
use amiss_scan::{Error, Resolution, ScanLimits, ScanResources, discover, discover_index};
use amiss_wire::controls::ResourceName;
use amiss_wire::digest::{hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::resolution::{BlobContent, BlobMode, Target, UnsupportedSemantics};

use crate::support::{POINTER, bed, bed_with, fixture, git};

#[test]
fn lfs_pointer_targets_resolve_with_pointer_availability() {
    let mut bed = bed();
    let (_i, row) = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "../pointer.bin",
        )
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = row
    else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("pointer.bin"));
    assert_eq!(blob.mode, BlobMode::Regular);
    let BlobContent::LfsPointer { raw_digest } = blob.content else {
        panic!("unexpected blob content: {:?}", blob.content);
    };
    assert_eq!(raw_digest, hb(RAW_EVIDENCE_DOMAIN, POINTER.as_bytes()));

    let selected = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "../pointer.bin#L1",
        )
        .unwrap_or_else(|_defect| panic!("resolve pointer selection"))
        .1;
    let Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(Target::Blob(
        selected,
    ))) = selected
    else {
        panic!("unexpected pointer-selection resolution: {selected:?}");
    };
    assert_eq!(
        selected.content,
        BlobContent::LfsPointer { raw_digest },
        "line evaluation must not reinterpret an LFS pointer as source bytes"
    );
}

#[test]
fn target_digests_recompute_exactly() {
    let mut bed = bed();
    let (_i, row) = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "data.json")
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = row
    else {
        panic!("unexpected resolution: {row:?}");
    };
    let BlobContent::Available {
        raw_digest,
        projection_digest,
    } = blob.content
    else {
        panic!("unexpected blob content: {:?}", blob.content);
    };
    let raw = hb(RAW_EVIDENCE_DOMAIN, b"{}\n");
    assert_eq!(raw_digest, raw);
    let projection = hj(
        TARGET_PROJECTION_DOMAIN,
        &Value::object(vec![
            ("git_mode".to_owned(), Value::string("100644".to_owned())),
            ("raw_digest".to_owned(), Value::string(raw.to_string())),
        ]),
    );
    assert_eq!(projection_digest, projection);
}

#[test]
fn targets_are_read_once_and_charged_once() {
    let mut bed = bed();
    let before = bed.scan_resources.target_bytes();
    let _first = bed.run_as(Adapter::Markdown, None, "docs/guide.md", false, "data.json");
    let after_first = bed.scan_resources.target_bytes();
    let _second = bed.run_as(
        Adapter::Markdown,
        None,
        "docs/guide.md",
        false,
        "./data.json",
    );
    let after_second = bed.scan_resources.target_bytes();
    assert_eq!(before, 0);
    assert_eq!(after_first, 3);
    assert_eq!(
        after_second, after_first,
        "the cache prevents a second charge"
    );
}

#[test]
fn a_reused_target_cache_tracks_object_and_scan_scope() {
    let mut bed = bed();
    let first = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "../src/lines.rs#L2",
        )
        .unwrap_or_else(|_defect| panic!("resolve first snapshot"))
        .1;
    let Resolution::Resolved {
        target: Target::Blob(first),
    } = first
    else {
        panic!("unexpected first resolution: {first:?}");
    };

    let changed = b"one\r\nchanged\nthree\rfour";
    fs::write(bed.dir.root().join("src/lines.rs"), changed)
        .unwrap_or_else(|_defect| panic!("write changed target"));
    git(bed.dir.root(), &["add", "src/lines.rs"]);
    git(bed.dir.root(), &["commit", "-qm", "change target"]);
    let tree = Oid::new(
        ObjectFormat::Sha1,
        git(bed.dir.root(), &["rev-parse", "HEAD^{tree}"])
            .trim()
            .to_owned(),
    )
    .unwrap_or_else(|| panic!("candidate tree identity"));
    let mut discovery_resources = ScanResources::new(ScanLimits::CONTRACT);
    bed.snapshot = discover(
        &bed.repo,
        &mut bed.git_resources,
        &mut discovery_resources,
        &amiss_scan::Includes::default(),
        &tree,
    )
    .unwrap_or_else(|_defect| panic!("discover changed snapshot"));

    let second = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "../src/lines.rs#L2",
        )
        .unwrap_or_else(|_defect| panic!("resolve changed snapshot"))
        .1;
    let Resolution::Resolved {
        target: Target::Blob(second),
    } = second
    else {
        panic!("unexpected changed resolution: {second:?}");
    };
    assert_ne!(
        first.content, second.content,
        "a new object at one path cannot reuse stale body or line evidence"
    );

    bed.scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let repeated = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "../src/lines.rs#L2",
        )
        .unwrap_or_else(|_defect| panic!("resolve in fresh scan scope"))
        .1;
    assert_eq!(
        repeated,
        Resolution::Resolved {
            target: Target::Blob(second)
        }
    );
    let changed_len = u64::try_from(changed.len()).unwrap_or(u64::MAX);
    assert_eq!(bed.scan_resources.target_bytes(), changed_len);
    assert_eq!(bed.scan_resources.line_fragment_bytes(), changed_len);
}

#[test]
fn target_budgets_bound_resolution() {
    let mut bed = bed_with(ScanLimits {
        referenced_target_blob_bytes: 2,
        ..ScanLimits::CONTRACT
    });
    let got = bed.run_as(Adapter::Markdown, None, "docs/guide.md", false, "data.json");
    assert_eq!(
        got,
        Err(Error::ResourceLimit {
            resource: ResourceName::ReferencedTargetBlobBytes,
            configured_limit: 2,
            observed_lower_bound: 3,
        })
    );

    let mut bed = bed_with(ScanLimits {
        aggregate_referenced_target_bytes_per_snapshot: 4,
        ..ScanLimits::CONTRACT
    });
    assert!(
        bed.run_as(Adapter::Markdown, None, "docs/guide.md", false, "data.json")
            .is_ok()
    );
    let crossing = bed.run_as(
        Adapter::Markdown,
        None,
        "docs/guide.md",
        false,
        "../src/lib.rs",
    );
    assert_eq!(
        crossing,
        Err(Error::ResourceLimit {
            resource: ResourceName::AggregateReferencedTargetBytesPerSnapshot,
            configured_limit: 4,
            observed_lower_bound: 16,
        })
    );
}

/// The same content must resolve the same way whichever candidate mode names
/// it. A commit tree carries a directory as an entry of its own; a Git index
/// carries only file paths, and a directory in it is exactly a path that some
/// entry lives under. An exact-entry lookup therefore saw directories in one
/// snapshot and not the other, and `[dir](./sub/)` (a terminal slash, which the
/// spec makes an authored directory hint, so `target_kind = tree`) resolved
/// through `--candidate` and came back missing through `--index`, on identical
/// bytes. The scanner found this in its own repository, on a directory link one
/// of its specification documents carried with the terminal slash.
#[test]
fn a_directory_resolves_the_same_through_a_commit_and_through_the_index() {
    let dir = fixture();
    let tree = Oid::new(
        ObjectFormat::Sha1,
        dir.commits.first().unwrap().tree.clone(),
    )
    .unwrap();
    let repo = Repository::open(dir.root(), ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let includes = amiss_scan::Includes::default();

    let from_tree = discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &tree,
    )
    .unwrap();
    let bytes = repo.read_index_bytes(&mut git_resources).unwrap();
    let index = amiss_git::parse_index_file(ObjectFormat::Sha1, &bytes).unwrap();
    let from_index = discover_index(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &index,
    )
    .unwrap();

    for reference in ["./sub/", "./sub", "./sub/keep.txt", "./nowhere/"] {
        let mut cache = TargetCache::default();
        let (tree_intent, tree_row) = Resolver::new(
            &repo,
            &mut git_resources,
            &mut scan_resources,
            &mut cache,
            &from_tree,
        )
        .resolve(
            None,
            Adapter::Markdown,
            &RepoPath::new("docs/guide.md".to_owned()).unwrap(),
            false,
            reference,
        )
        .unwrap();
        let mut cache = TargetCache::default();
        let (index_intent, index_row) = Resolver::new(
            &repo,
            &mut git_resources,
            &mut scan_resources,
            &mut cache,
            &from_index,
        )
        .resolve(
            None,
            Adapter::Markdown,
            &RepoPath::new("docs/guide.md".to_owned()).unwrap(),
            false,
            reference,
        )
        .unwrap();
        assert_eq!(tree_intent, index_intent, "intent for {reference}");
        assert_eq!(
            tree_row, index_row,
            "the index and the commit hold the same content, so {reference} resolves the same"
        );
    }
}
