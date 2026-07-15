use std::fs;
use std::path::Path;

use amiss_fixtures::stage_symlink;
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::resolve::{
    ForgeContext, RAW_EVIDENCE_DOMAIN, TARGET_PROJECTION_DOMAIN, TargetCache,
};
use amiss_scan::{
    Error, ScanLimits, ScanResources, SnapshotDiscovery, discover, discover_index, resolve,
};
use amiss_wire::controls::{ContentAvailability, EntryKind, GitMode, ResourceName, TargetKind};
use amiss_wire::digest::{hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::ForgeDialect;
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::{IntentKind, ResolutionCode};
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

const POINTER: &str = "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n";

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README"), "root doc\n").unwrap();
    fs::write(root.join("llms.txt"), "advisory\n").unwrap();
    fs::write(root.join("pointer.bin"), POINTER).unwrap();
    fs::create_dir_all(root.join("docs/sub")).unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
    fs::write(root.join("docs/data.json"), "{}\n").unwrap();
    fs::write(root.join("docs/sub/keep.txt"), "kept\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("vendor/inside.md"), "hidden\n").unwrap();
    git(root, &["add", "."]);
    stage_symlink(root, "README", "alias").unwrap();
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,0123456789012345678901234567890123456789,module",
        ],
    );
    // Staged as exact bytes rather than written to disk, because a macOS worktree
    // would hand back the decomposed spelling and the fixture would be testing the
    // filesystem instead of the resolver.
    let blob = git(root, &["rev-parse", ":docs/sub/keep.txt"])
        .trim()
        .to_owned();
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},docs/\u{e9}t\u{e9}.txt"),
        ],
    );
    git(root, &["commit", "-qm", "fixture"]);
    dir
}

struct Bed {
    _dir: TempDir,
    repo: Repository,
    git_resources: GitResources,
    scan_resources: ScanResources,
    cache: TargetCache,
    snapshot: SnapshotDiscovery,
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn bed_with(limits: ScanLimits) -> Bed {
    let dir = fixture();
    let hex = git(dir.path(), &["rev-parse", "HEAD^{tree}"])
        .trim()
        .to_owned();
    let tree = Oid::new(ObjectFormat::Sha1, hex).unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let snapshot = discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &amiss_scan::Includes::default(),
        &tree,
    )
    .unwrap();
    Bed {
        _dir: dir,
        repo,
        git_resources,
        scan_resources: ScanResources::new(limits),
        cache: TargetCache::default(),
        snapshot,
    }
}

fn bed() -> Bed {
    bed_with(ScanLimits::CONTRACT)
}

fn github_context() -> ForgeContext {
    ForgeContext {
        host: "github.com".to_owned(),
        dialect: ForgeDialect::Github,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/feature/x".to_owned(),
        default_ref: "refs/heads/main".to_owned(),
        candidate_oid: None,
    }
}

impl Bed {
    fn run(
        &mut self,
        context: Option<&ForgeContext>,
        document: &str,
        is_image: bool,
        destination: &str,
    ) -> Result<(amiss_scan::Intent, amiss_scan::Resolution), Error> {
        #[expect(clippy::unwrap_used, reason = "test fixture helper")]
        let document = RepoPath::new(document.to_owned()).unwrap();
        resolve(
            &self.repo,
            &mut self.git_resources,
            &mut self.scan_resources,
            &mut self.cache,
            &self.snapshot,
            context,
            &document,
            is_image,
            destination,
        )
    }

    #[expect(clippy::expect_used, reason = "test fixture helper")]
    fn code(&mut self, destination: &str) -> ResolutionCode {
        self.run(None, "docs/guide.md", false, destination)
            .expect("resolve")
            .1
            .code
    }
}

#[test]
fn component_splitting_follows_rfc_order() {
    let mut bed = bed();
    let (intent, row) = bed
        .run(None, "docs/guide.md", false, "https://e.com/a?x?y#z?u")
        .unwrap_or_else(|_defect| panic!("resolve"));
    assert_eq!(row.code, ResolutionCode::ExternalUrl);
    assert_eq!(intent.kind, IntentKind::ExternalUrl);
    assert_eq!(intent.external_scheme.as_deref(), Some("https"));
    assert_eq!(intent.query.as_deref(), Some("x?y"));
    assert_eq!(intent.fragment.as_deref(), Some("z?u"));
}

#[test]
fn schemes_classify_external_and_uris_validate() {
    let mut bed = bed();
    assert_eq!(bed.code("MAILTO:a@b.example"), ResolutionCode::ExternalUrl);
    assert_eq!(bed.code("custom+x.y:anything"), ResolutionCode::ExternalUrl);
    assert_eq!(bed.code("https:no-authority"), ResolutionCode::InvalidUri);
    assert_eq!(bed.code("https://"), ResolutionCode::InvalidUri);
    assert_eq!(bed.code("https://e.com/a b"), ResolutionCode::InvalidUri);
    assert_eq!(
        bed.code("https://ex\u{e4}mple.com/x"),
        ResolutionCode::InvalidUri
    );
    assert_eq!(bed.code("https://e.com/a%zz"), ResolutionCode::InvalidUri);
    assert_eq!(
        bed.code("//cdn.e.com/x"),
        ResolutionCode::NetworkPathUnsupported
    );
    assert_eq!(
        bed.code("/guide/start"),
        ResolutionCode::SiteRouteUnsupported
    );
}

#[test]
fn native_paths_decode_once_and_stay_contained() {
    let mut bed = bed();
    assert_eq!(bed.code("../../x.md"), ResolutionCode::PathTraversal);
    assert_eq!(bed.code("a%2Fb.md"), ResolutionCode::EncodedSlash);
    assert_eq!(bed.code("%5Cx"), ResolutionCode::BackslashSeparator);
    assert_eq!(bed.code("a\\b.md"), ResolutionCode::BackslashSeparator);
    assert_eq!(bed.code("a%zz.md"), ResolutionCode::InvalidPercentEncoding);
    assert_eq!(bed.code("a%00b.md"), ResolutionCode::DecodedPathControl);
    assert_eq!(bed.code("a//b.md"), ResolutionCode::InvalidReference);
    assert_eq!(bed.code("sub//"), ResolutionCode::InvalidReference);
    assert_eq!(bed.code("guide.md"), ResolutionCode::ExactPath);
    assert_eq!(bed.code("./guide.md"), ResolutionCode::ExactPath);
    assert_eq!(bed.code("%2E%2E/README"), ResolutionCode::ExactPath);
    assert_eq!(bed.code("absent.md"), ResolutionCode::PathNotFound);

    // `%25` decodes to a literal `%` and stops there. A second pass is what turns
    // `%252E%252E/` into `../` and `%252F` into a separator, so the whole defence
    // is that the pass never happens: each of these is a filename with per cent
    // signs in it, and none of them is a path.
    assert_eq!(bed.code("%252E%252E/README"), ResolutionCode::PathNotFound);
    assert_eq!(bed.code("docs%252Fguide.md"), ResolutionCode::PathNotFound);
    assert_eq!(bed.code("a%252Fb.md"), ResolutionCode::PathNotFound);
}

#[test]
fn terminal_slashes_author_trees_and_break_images() {
    let mut bed = bed();
    let (intent, row) = bed
        .run(None, "docs/guide.md", false, "sub/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.target_kind, Some(TargetKind::Tree));
    assert_eq!(row.code, ResolutionCode::ExactPath);
    assert_eq!(row.entry_kind, Some(EntryKind::Tree));
    assert_eq!(row.git_mode, Some(GitMode::Tree));
    assert_eq!(row.content_availability, ContentAvailability::NotApplicable);
    assert_eq!(row.raw_digest, None);

    let (_intent, image) = bed
        .run(None, "docs/guide.md", true, "sub/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(image.code, ResolutionCode::InvalidReference);

    let (intent, mismatch) = bed
        .run(None, "docs/guide.md", false, "guide.md/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.target_kind, Some(TargetKind::Tree));
    assert_eq!(mismatch.code, ResolutionCode::TargetTypeMismatch);
    assert_eq!(mismatch.entry_kind, Some(EntryKind::Blob));
    assert_eq!(
        mismatch.content_availability,
        ContentAvailability::Available
    );
    assert!(mismatch.raw_digest.is_some() && mismatch.projection_digest.is_some());
}

#[test]
fn special_entries_are_never_followed() {
    let mut bed = bed();
    let (_i, sym) = bed
        .run(None, "docs/guide.md", false, "../alias")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(
        (
            sym.code,
            sym.entry_kind,
            sym.git_mode,
            sym.content_availability
        ),
        (
            ResolutionCode::SymlinkEntry,
            Some(EntryKind::Symlink),
            Some(GitMode::Symlink),
            ContentAvailability::NotRead
        )
    );
    let (_i, gitlink) = bed
        .run(None, "docs/guide.md", false, "../module")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(gitlink.code, ResolutionCode::GitlinkEntry);
    assert_eq!(gitlink.git_mode, Some(GitMode::Gitlink));
}

#[test]
fn empty_destinations_target_the_source_document() {
    let mut bed = bed();
    assert_eq!(bed.code(""), ResolutionCode::ExactPath);
    assert_eq!(bed.code("?q"), ResolutionCode::ExactPath);
    assert_eq!(
        bed.code("#Intro"),
        ResolutionCode::UnsupportedFragmentSemantics
    );
    assert_eq!(bed.code("#L5"), ResolutionCode::CodeFragmentUnevaluated);
    assert_eq!(bed.code("#"), ResolutionCode::ExactPath);
}

#[test]
fn query_and_fragment_semantics_follow_the_precedence() {
    let mut bed = bed();
    assert_eq!(
        bed.code("data.json?x"),
        ResolutionCode::UnsupportedQuerySemantics
    );
    assert_eq!(
        bed.code("data.json?x#sym"),
        ResolutionCode::UnsupportedQuerySemantics
    );
    assert_eq!(
        bed.code("guide.md?x#Intro"),
        ResolutionCode::UnsupportedFragmentSemantics
    );
    assert_eq!(bed.code("guide.md?x"), ResolutionCode::ExactPath);
    assert_eq!(
        bed.code("../vendor/inside.md?x"),
        ResolutionCode::UnsupportedQuerySemantics
    );
    assert_eq!(
        bed.code("../llms.txt?x"),
        ResolutionCode::UnsupportedQuerySemantics
    );
    assert_eq!(
        bed.code("data.json#anything"),
        ResolutionCode::CodeFragmentUnevaluated
    );
    assert_eq!(
        bed.code("guide.md#%zz"),
        ResolutionCode::InvalidFragmentEncoding
    );

    let (_i, retained) = bed
        .run(None, "docs/guide.md", false, "data.json?x")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(retained.entry_kind, Some(EntryKind::Blob));
    assert_eq!(
        retained.content_availability,
        ContentAvailability::Available
    );
    assert!(retained.raw_digest.is_some());
}

#[test]
fn line_fragments_have_a_hard_grammar() {
    let mut bed = bed();
    assert_eq!(
        bed.code("guide.md#L1"),
        ResolutionCode::CodeFragmentUnevaluated
    );
    assert_eq!(
        bed.code("guide.md#L1-L1"),
        ResolutionCode::CodeFragmentUnevaluated
    );
    assert_eq!(
        bed.code("guide.md#L10-L20"),
        ResolutionCode::CodeFragmentUnevaluated
    );
    for renderer in ["L0", "l5", "L5-L2", "L", "L5x", "L05"] {
        assert_eq!(
            bed.code(&format!("guide.md#{renderer}")),
            ResolutionCode::UnsupportedFragmentSemantics,
            "{renderer} is not the line grammar, and the target is a document"
        );
    }
}

#[test]
fn lfs_pointer_targets_resolve_with_pointer_availability() {
    let mut bed = bed();
    let (_i, row) = bed
        .run(None, "docs/guide.md", false, "../pointer.bin")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(row.code, ResolutionCode::ExactPath);
    assert_eq!(
        row.content_availability,
        ContentAvailability::LfsPointerOnly
    );
    assert_eq!(
        row.raw_digest,
        Some(hb(RAW_EVIDENCE_DOMAIN, POINTER.as_bytes()))
    );
    assert_eq!(row.projection_digest, None);
}

#[test]
fn target_digests_recompute_exactly() {
    let mut bed = bed();
    let (_i, row) = bed
        .run(None, "docs/guide.md", false, "data.json")
        .unwrap_or_else(|_d| panic!());
    let raw = hb(RAW_EVIDENCE_DOMAIN, b"{}\n");
    assert_eq!(row.raw_digest, Some(raw));
    let projection = hj(
        TARGET_PROJECTION_DOMAIN,
        &Value::Object(vec![
            ("git_mode".to_owned(), Value::String("100644".to_owned())),
            ("raw_digest".to_owned(), Value::String(raw.to_string())),
        ]),
    );
    assert_eq!(row.projection_digest, Some(projection));
}

#[test]
fn targets_are_read_once_and_charged_once() {
    let mut bed = bed();
    let before = bed.scan_resources.target_bytes();
    let _first = bed.run(None, "docs/guide.md", false, "data.json");
    let after_first = bed.scan_resources.target_bytes();
    let _second = bed.run(None, "docs/guide.md", false, "./data.json");
    let after_second = bed.scan_resources.target_bytes();
    assert_eq!(before, 0);
    assert_eq!(after_first, 3);
    assert_eq!(
        after_second, after_first,
        "the cache prevents a second charge"
    );
}

#[test]
fn target_budgets_bound_resolution() {
    let mut bed = bed_with(ScanLimits {
        referenced_target_blob_bytes: 2,
        ..ScanLimits::CONTRACT
    });
    let got = bed.run(None, "docs/guide.md", false, "data.json");
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
    assert!(bed.run(None, "docs/guide.md", false, "data.json").is_ok());
    let crossing = bed.run(None, "docs/guide.md", false, "../src/lib.rs");
    assert_eq!(
        crossing,
        Err(Error::ResourceLimit {
            resource: ResourceName::AggregateReferencedTargetBytesPerSnapshot,
            configured_limit: 4,
            observed_lower_bound: 16,
        })
    );
}

#[test]
fn github_urls_need_the_whole_trusted_chain() {
    let mut bed = bed();
    let context = github_context();

    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGithub);
    assert_eq!(
        intent.repository_path.as_ref().and_then(RepoPath::as_str),
        Some("docs/guide.md")
    );
    assert_eq!(intent.target_kind, Some(TargetKind::Blob));
    assert_eq!(row.code, ResolutionCode::ExactPath);

    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/ACME/widgets/blob/main/docs/guide.md",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGithub);
    assert_eq!(row.code, ResolutionCode::UnsupportedVersionScope);
    assert_eq!(
        row.path.as_ref().and_then(RepoPath::as_str),
        Some("docs/guide.md")
    );
    assert_eq!(
        row.entry_kind, None,
        "a default-only split is never looked up"
    );
}

#[test]
fn github_without_trust_is_foreign() {
    let mut bed = bed();
    let context = github_context();
    let (_i, foreign) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/other/widgets/blob/main/x",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(foreign.code, ResolutionCode::ForeignRepository);
    let (_i, no_context) = bed
        .run(
            None,
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(no_context.code, ResolutionCode::ForeignRepository);

    assert_eq!(
        bed.run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x"
        )
        .unwrap_or_else(|_d| panic!())
        .1
        .code,
        ResolutionCode::InvalidReference,
        "a ref consuming the complete suffix leaves no path"
    );
    assert_eq!(
        bed.run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/main/../x"
        )
        .unwrap_or_else(|_d| panic!())
        .1
        .code,
        ResolutionCode::PathTraversal
    );
    assert_eq!(
        bed.run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/nope/x"
        )
        .unwrap_or_else(|_d| panic!())
        .1
        .code,
        ResolutionCode::UnsupportedVersionScope
    );
    assert_eq!(
        bed.run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/a%2Fb"
        )
        .unwrap_or_else(|_d| panic!())
        .1
        .code,
        ResolutionCode::EncodedSlash
    );

    let (_i, tree) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/tree/feature/x/docs/",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(tree.code, ResolutionCode::ExactPath);
    assert_eq!(tree.entry_kind, Some(EntryKind::Tree));

    let (_i, lines) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/src/lib.rs#L10-L20",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(lines.code, ResolutionCode::CodeFragmentUnevaluated);
    assert_eq!(lines.content_availability, ContentAvailability::Available);

    let (_i, tree_fragment) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/tree/feature/x/docs#readme",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(tree_fragment.code, ResolutionCode::CodeFragmentUnevaluated);
}

#[test]
fn ambiguous_trusted_splits_are_version_scope_with_null_fields() {
    let mut bed = bed();
    let context = ForgeContext {
        host: "github.com".to_owned(),
        dialect: ForgeDialect::Github,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/a".to_owned(),
        default_ref: "refs/heads/a/b".to_owned(),
        candidate_oid: None,
    };
    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/a/b/c",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.kind, IntentKind::Unsupported);
    assert_eq!(row.code, ResolutionCode::UnsupportedVersionScope);
    assert_eq!(row.path, None);
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
    let hex = git(dir.path(), &["rev-parse", "HEAD^{tree}"])
        .trim()
        .to_owned();
    let tree = Oid::new(ObjectFormat::Sha1, hex).unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
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
        let (tree_intent, tree_row) = resolve(
            &repo,
            &mut git_resources,
            &mut scan_resources,
            &mut cache,
            &from_tree,
            None,
            &RepoPath::new("docs/guide.md".to_owned()).unwrap(),
            false,
            reference,
        )
        .unwrap();
        let mut cache = TargetCache::default();
        let (index_intent, index_row) = resolve(
            &repo,
            &mut git_resources,
            &mut scan_resources,
            &mut cache,
            &from_index,
            None,
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

/// A Git tree names its entries in bytes, and the resolver compares them as bytes.
/// It does not case-fold, and it does not normalize Unicode, so `Guide.md` is not
/// `guide.md` and the precomposed spelling of an accent is not the decomposed one.
/// Both temptations lead the same way: fold either, and a reference that points at
/// nothing starts resolving against a file that merely looks like its target, which
/// retires a real broken link into a silent pass. The risk is not theoretical. This
/// suite runs on macOS, whose filesystem case-folds and hands back decomposed names,
/// so a resolver that ever reached for the disk instead of the tree would go green
/// there and stay red nowhere.
#[test]
fn paths_are_bytes_and_the_resolver_neither_folds_case_nor_normalizes_them() {
    let mut bed = bed();

    assert_eq!(bed.code("guide.md"), ResolutionCode::ExactPath);
    assert_eq!(bed.code("Guide.md"), ResolutionCode::PathNotFound);
    assert_eq!(bed.code("GUIDE.MD"), ResolutionCode::PathNotFound);
    assert_eq!(bed.code("../README"), ResolutionCode::ExactPath);
    assert_eq!(bed.code("../readme"), ResolutionCode::PathNotFound);

    // U+00E9, the precomposed accent the tree actually carries.
    assert_eq!(bed.code("\u{e9}t\u{e9}.txt"), ResolutionCode::ExactPath);
    // The same two accents decomposed into e + U+0301: the same text, other bytes.
    assert_eq!(
        bed.code("e\u{301}te\u{301}.txt"),
        ResolutionCode::PathNotFound
    );
}

/// The recognition opening requires the exact path separator after the
/// declared host: a host-prefixed lookalike authority is a different site,
/// external rather than a foreign form of this one.
#[test]
fn a_host_prefix_lookalike_authority_stays_external() {
    let mut bed = bed();
    let context = github_context();
    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com.evil.example/acme/widgets/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(row.code, ResolutionCode::ExternalUrl);
    assert_eq!(intent.kind, IntentKind::ExternalUrl);
}

fn gitlab_context() -> ForgeContext {
    ForgeContext {
        host: "gitlab.com".to_owned(),
        dialect: ForgeDialect::Gitlab,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/feature/x".to_owned(),
        default_ref: "refs/heads/main".to_owned(),
        candidate_oid: None,
    }
}

/// The gitlab dialect against a real tree: the canonical separator form
/// resolves, an encoded owner segment is foreign, and a ref matching
/// neither trusted ref is version-scoped out with its path disclosed.
#[test]
fn gitlab_recognition_resolves_against_the_tree() {
    let mut bed = bed();
    let context = gitlab_context();
    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGitlab);
    assert_eq!(row.code, ResolutionCode::ExactPath);

    let (_intent, encoded) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acm%65/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(encoded.code, ResolutionCode::ForeignRepository);

    let (_intent, pinned) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/blob/0123456789012345678901234567890123456789/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        pinned.code,
        ResolutionCode::UnsupportedVersionScope,
        "a commit-pinned link matches neither trusted ref, exactly as on github"
    );
    assert_eq!(pinned.path, None);
}
