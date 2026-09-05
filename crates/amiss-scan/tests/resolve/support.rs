use std::path::Path;

use amiss_fixtures::{CommitChain, Staged, staged_repository};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::resolve::{ForgeContext, Resolver, TargetCache};
use amiss_scan::{Error, Resolution, ScanLimits, ScanResources, SnapshotDiscovery, discover};
use amiss_wire::controls::TargetKind;
use amiss_wire::model::ForgeDialect;
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    BlobContent, ExternalReference, InvalidReference, Target, UnsupportedSemantics, VersionScope,
};

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

pub(crate) const POINTER: &str = "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n";

pub(crate) const ANCHORS: &[u8] = "# Setup & Config\n\n## Setup & Config\n\n### Résumé draft\n\n<a name=\"declared\"></a>\n\n## Explicit {#custom}\n\n<h2 align=\"center\"><code>tool</code></h2>\n\n[](){#anchor-point}\n\n## Pair { id=\"pair-id\" }\n".as_bytes();

pub(crate) const MIXED_LINES: &[u8] = b"one\r\ntwo\nthree\rfour";

pub(crate) const MIXED_LINES_OUTSIDE_CHANGED: &[u8] =
    b"changed before\r\ntwo\nchanged after\rchanged tail";

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn fixture() -> CommitChain {
    staged_repository(&[
        ("README", Staged::File(b"root doc\n")),
        ("alias", Staged::Symlink("README")),
        ("llms.txt", Staged::File(b"advisory\n")),
        ("pointer.bin", Staged::File(POINTER.as_bytes())),
        ("docs/guide.md", Staged::File(b"# Guide\n")),
        ("docs/anchors.md", Staged::File(ANCHORS)),
        (
            "docs/case.md",
            Staged::File(b"<a name=\"Dup\"></a>\n<a name=\"dup\"></a>\n"),
        ),
        ("docs/pointer.md", Staged::File(POINTER.as_bytes())),
        ("docs/invalid.md", Staged::File(b"# \xff\n")),
        ("docs/data.json", Staged::File(b"{}\n")),
        ("docs/sub/keep.txt", Staged::File(b"kept\n")),
        ("docs/sub/README.md", Staged::File(b"# Sub\n")),
        // Staged without a worktree file, because a macOS worktree would hand back
        // the decomposed spelling and the fixture would be testing the filesystem.
        ("docs/\u{e9}t\u{e9}.txt", Staged::Absent(b"kept\n")),
        ("src/lib.rs", Staged::File(b"fn main() {}\n")),
        ("src/lines.rs", Staged::File(MIXED_LINES)),
        ("src/executable.sh", Staged::Executable(MIXED_LINES)),
        (
            "src/lines-outside-changed.rs",
            Staged::File(MIXED_LINES_OUTSIDE_CHANGED),
        ),
        ("src/empty.rs", Staged::File(b"")),
        ("vendor/inside.md", Staged::File(b"hidden\n")),
        (
            "module",
            Staged::Submodule("0123456789012345678901234567890123456789"),
        ),
    ])
    .unwrap()
}

pub(crate) struct Bed {
    pub(crate) dir: CommitChain,
    pub(crate) repo: Repository,
    pub(crate) git_resources: GitResources,
    pub(crate) scan_resources: ScanResources,
    pub(crate) cache: TargetCache,
    pub(crate) snapshot: SnapshotDiscovery,
}

pub(crate) fn bed_with(limits: ScanLimits) -> Bed {
    bed_at(fixture(), 0, limits, GitLimits::CONTRACT)
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
pub(crate) fn bed_at(
    dir: CommitChain,
    commit: usize,
    scan_limits: ScanLimits,
    git_limits: GitLimits,
) -> Bed {
    let tree = Oid::new(
        ObjectFormat::Sha1,
        dir.commits.get(commit).unwrap().tree.clone(),
    )
    .unwrap();
    let repo = Repository::open(dir.root(), ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(git_limits);
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
        dir,
        repo,
        git_resources,
        scan_resources: ScanResources::new(scan_limits),
        cache: TargetCache::default(),
        snapshot,
    }
}

pub(crate) fn bed() -> Bed {
    bed_with(ScanLimits::CONTRACT)
}

pub(crate) fn forge_context(dialect: ForgeDialect) -> ForgeContext {
    let (host, candidate_ref) = match dialect {
        ForgeDialect::Github => ("github.com", "refs/heads/feature/x"),
        ForgeDialect::Gitlab => ("gitlab.com", "refs/heads/feature/x"),
        ForgeDialect::Gitea => ("codeberg.org", "refs/heads/feature/x"),
        ForgeDialect::BitbucketCloud => ("bitbucket.org", "refs/heads/feature"),
        ForgeDialect::BitbucketDataCenter => ("bitbucket.example", "refs/heads/feature/x"),
    };
    ForgeContext {
        host: host.to_owned(),
        dialect,
        object_format: ObjectFormat::Sha1,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: candidate_ref.to_owned(),
        default_ref: "refs/heads/main".to_owned(),
    }
}

impl Bed {
    pub(crate) fn resolver(&mut self) -> Resolver<'_> {
        Resolver::new(
            &self.repo,
            &mut self.git_resources,
            &mut self.scan_resources,
            &mut self.cache,
            &self.snapshot,
        )
    }

    pub(crate) fn run_as(
        &mut self,
        adapter: Adapter,
        context: Option<&ForgeContext>,
        document: &str,
        is_image: bool,
        destination: &str,
    ) -> Result<(amiss_scan::Intent, Resolution), Error> {
        #[expect(clippy::unwrap_used, reason = "test fixture helper")]
        let document = RepoPath::new(document.to_owned()).unwrap();
        self.resolver()
            .resolve(context, adapter, &document, is_image, destination)
    }
}

#[test]
pub(crate) fn github_urls_need_the_whole_trusted_chain() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Github);

    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
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
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = row
    else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/ACME/widgets/blob/main/docs/guide.md?plain=1#intro",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGithub);
    assert_eq!(intent.query.as_deref(), Some("plain=1"));
    assert_eq!(intent.fragment.as_deref(), Some("intro"));
    let Resolution::UnsupportedVersion(VersionScope::KnownPath { path }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/guide.md"));

    let commit = "0123456789012345678901234567890123456789";
    let (_intent, row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            &format!("https://github.com/acme/widgets/blob/{commit}/docs/guide.md"),
        )
        .unwrap_or_else(|_d| panic!());
    let Resolution::UnsupportedVersion(VersionScope::KnownCommit { commit_oid, path }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(commit_oid.as_str(), commit);
    assert_eq!(path.as_str(), Some("docs/guide.md"));
}

#[test]
fn github_with_a_different_trusted_identity_is_foreign() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Github);
    let (intent, foreign) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/other/widgets/blob/main/x",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.kind, IntentKind::ExternalUrl);
    assert_eq!(
        foreign,
        Resolution::External(ExternalReference::ForeignRepository)
    );

    let row = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x",
        )
        .unwrap_or_else(|_d| panic!())
        .1;
    assert_eq!(
        row,
        Resolution::Invalid(InvalidReference::Syntax),
        "a ref consuming the complete suffix leaves no path"
    );

    let row = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/main/../x",
        )
        .unwrap_or_else(|_d| panic!())
        .1;
    assert_eq!(row, Resolution::Invalid(InvalidReference::PathTraversal));

    let row = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/nope/x",
        )
        .unwrap_or_else(|_d| panic!())
        .1;
    assert!(
        matches!(&row, Resolution::UnsupportedVersion(_)),
        "unexpected resolution: {row:?}"
    );

    let row = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/a%2Fb",
        )
        .unwrap_or_else(|_d| panic!())
        .1;
    assert_eq!(row, Resolution::Invalid(InvalidReference::EncodedSlash));
}

#[test]
fn github_candidate_urls_resolve_targets_and_fragments() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Github);
    let (_i, tree) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/tree/feature/x/docs/",
        )
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved {
        target: Target::Tree { path },
    } = tree
    else {
        panic!("unexpected resolution: {tree:?}");
    };
    assert_eq!(path.as_str(), Some("docs"));

    let (_i, lines) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/src/lib.rs#L1-L1",
        )
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = lines
    else {
        panic!("unexpected resolution: {lines:?}");
    };
    assert_eq!(blob.path.as_str(), Some("src/lib.rs"));
    assert!(matches!(blob.content, BlobContent::Available { .. }));

    let (_i, tree_fragment) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/tree/feature/x/docs#readme",
        )
        .unwrap_or_else(|_d| panic!());
    let Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(Target::Tree { path })) =
        tree_fragment
    else {
        panic!("unexpected resolution: {tree_fragment:?}");
    };
    assert_eq!(path.as_str(), Some("docs"));
}

#[test]
fn ambiguous_trusted_splits_have_unknown_version_scope() {
    let mut bed = bed();
    let context = ForgeContext {
        host: "github.com".to_owned(),
        dialect: ForgeDialect::Github,
        object_format: ObjectFormat::Sha1,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/a".to_owned(),
        default_ref: "refs/heads/a/b".to_owned(),
    };
    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/a/b/c",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.kind, IntentKind::Unsupported);
    assert_eq!(
        row,
        Resolution::UnsupportedVersion(VersionScope::UnknownPath)
    );
}

#[test]
fn forge_urls_without_a_declared_context_are_external() {
    let mut bed = bed();
    let urls = [
        "https://github.com/acme/widgets/blob/feature/x/docs/guide.md",
        "https://gitlab.com/acme/widgets/-/blob/feature/x/docs/guide.md",
        "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/guide.md",
        "https://bitbucket.org/acme/widgets/src/feature/docs/guide.md",
    ];

    for url in urls {
        let (intent, row) = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, url)
            .unwrap_or_else(|_defect| panic!());
        assert_eq!(intent.kind, IntentKind::ExternalUrl, "{url}");
        assert_eq!(intent.external_scheme.as_deref(), Some("https"), "{url}");
        assert_eq!(row, Resolution::External(ExternalReference::Url), "{url}");
    }
}

/// The recognition opening requires the exact path separator after the
/// declared host: a host-prefixed lookalike authority is a different site,
/// external rather than a foreign form of this one.
#[test]
fn a_host_prefix_lookalike_authority_stays_external() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Github);
    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com.evil.example/acme/widgets/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(row, Resolution::External(ExternalReference::Url));
    assert_eq!(intent.kind, IntentKind::ExternalUrl);
}

/// The gitlab dialect against a real tree: the canonical separator form
/// resolves, an encoded owner segment is foreign, and a ref matching
/// neither trusted ref is version-scoped out with its path disclosed.
#[test]
pub(crate) fn gitlab_recognition_resolves_against_the_tree() {
    let mut bed = bed();
    let context = forge_context(ForgeDialect::Gitlab);
    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGitlab);
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = row
    else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let (_intent, encoded) = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acm%65/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        encoded,
        Resolution::External(ExternalReference::ForeignRepository)
    );

    let (_intent, pinned) = bed
        .run_as(Adapter::Markdown,
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/blob/0123456789012345678901234567890123456789/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    let Resolution::UnsupportedVersion(VersionScope::KnownCommit { commit_oid, path }) = pinned
    else {
        panic!("unexpected resolution: {pinned:?}");
    };
    assert_eq!(
        commit_oid.as_str(),
        "0123456789012345678901234567890123456789"
    );
    assert_eq!(path.as_str(), Some("docs/guide.md"));
}
