use std::fs;
use std::path::Path;

use amiss_fixtures::stage_symlink;
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::resolve::{
    ForgeContext, RAW_EVIDENCE_DOMAIN, TARGET_LINE_PROJECTION_DOMAIN, TARGET_PROJECTION_DOMAIN,
    TargetCache,
};
use amiss_scan::{
    Error, Resolution, ScanLimits, ScanResources, SnapshotDiscovery, discover, discover_index,
    resolve,
};
use amiss_wire::controls::{GitMode, ResourceName, TargetKind};
use amiss_wire::digest::{hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::ForgeDialect;
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    BlobContent, BlobMode, ExternalReference, InvalidReference, Missing, Target,
    UnsupportedSemantics, UnsupportedTarget, VersionScope,
};
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

const POINTER: &str = "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n";
const MIXED_LINES: &[u8] = b"one\r\ntwo\nthree\rfour";
const MIXED_LINES_OUTSIDE_CHANGED: &[u8] = b"changed before\r\ntwo\nchanged after\rchanged tail";

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
    fs::write(root.join("src/lines.rs"), MIXED_LINES).unwrap();
    fs::write(root.join("src/executable.sh"), MIXED_LINES).unwrap();
    fs::write(
        root.join("src/lines-outside-changed.rs"),
        MIXED_LINES_OUTSIDE_CHANGED,
    )
    .unwrap();
    fs::write(root.join("src/empty.rs"), b"").unwrap();
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("vendor/inside.md"), "hidden\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["update-index", "--chmod=+x", "src/executable.sh"]);
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
    dir: TempDir,
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
        dir,
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
    ) -> Result<(amiss_scan::Intent, Resolution), Error> {
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
}

#[test]
fn component_splitting_follows_rfc_order() {
    let mut bed = bed();
    let (intent, row) = bed
        .run(None, "docs/guide.md", false, "https://e.com/a?x?y#z?u")
        .unwrap_or_else(|_defect| panic!("resolve"));
    assert!(matches!(row, Resolution::External(ExternalReference::Url)));
    assert_eq!(intent.kind, IntentKind::ExternalUrl);
    assert_eq!(intent.external_scheme.as_deref(), Some("https"));
    assert_eq!(intent.query.as_deref(), Some("x?y"));
    assert_eq!(intent.fragment.as_deref(), Some("z?u"));
}

#[test]
fn schemes_classify_external_and_uris_validate() {
    let mut bed = bed();
    for destination in ["MAILTO:a@b.example", "custom+x.y:anything"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::External(ExternalReference::Url)),
            "{destination}: {row:?}"
        );
    }
    for destination in [
        "https:no-authority",
        "https://",
        "https://e.com/a b",
        "https://ex\u{e4}mple.com/x",
        "https://e.com/a%zz",
    ] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Invalid(InvalidReference::Uri)),
            "{destination}: {row:?}"
        );
    }

    let row = bed
        .run(None, "docs/guide.md", false, "//cdn.e.com/x")
        .unwrap_or_else(|_defect| panic!("resolve network path"))
        .1;
    assert!(matches!(
        row,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::NetworkPath)
    ));

    let row = bed
        .run(None, "docs/guide.md", false, "/guide/start")
        .unwrap_or_else(|_defect| panic!("resolve site route"))
        .1;
    assert!(matches!(
        row,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute)
    ));
}

#[test]
fn native_paths_decode_once_and_stay_contained() {
    let mut bed = bed();
    for (destination, reason) in [
        ("../../x.md", InvalidReference::PathTraversal),
        ("a%2Fb.md", InvalidReference::EncodedSlash),
        ("%5Cx", InvalidReference::BackslashSeparator),
        ("a\\b.md", InvalidReference::BackslashSeparator),
        ("a%zz.md", InvalidReference::PercentEncoding),
        ("a%00b.md", InvalidReference::DecodedPathControl),
        ("a//b.md", InvalidReference::Syntax),
        ("sub//", InvalidReference::Syntax),
    ] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert_eq!(row, Resolution::Invalid(reason), "{destination}");
    }
    for destination in ["guide.md", "./guide.md", "%2E%2E/README"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Resolved(_)),
            "{destination}: {row:?}"
        );
    }
    let row = bed
        .run(None, "docs/guide.md", false, "absent.md")
        .unwrap_or_else(|_defect| panic!("resolve absent path"))
        .1;
    let Resolution::Missing(Missing::PathNotFound { path }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/absent.md"));

    // `%25` decodes to a literal `%` and stops there. A second pass is what turns
    // `%252E%252E/` into `../` and `%252F` into a separator, so the whole defence
    // is that the pass never happens: each of these is a filename with per cent
    // signs in it, and none of them is a path.
    for destination in ["%252E%252E/README", "docs%252Fguide.md", "a%252Fb.md"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Missing(Missing::PathNotFound { .. })),
            "{destination}: {row:?}"
        );
    }
}

#[test]
fn terminal_slashes_author_trees_and_break_images() {
    let mut bed = bed();
    let (intent, row) = bed
        .run(None, "docs/guide.md", false, "sub/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.target_kind, Some(TargetKind::Tree));
    let Resolution::Resolved(Target::Tree { path }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/sub"));

    let (_intent, image) = bed
        .run(None, "docs/guide.md", true, "sub/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(image, Resolution::Invalid(InvalidReference::Syntax));

    let (intent, mismatch) = bed
        .run(None, "docs/guide.md", false, "guide.md/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.target_kind, Some(TargetKind::Tree));
    let Resolution::TypeMismatch(Target::Blob(blob)) = mismatch else {
        panic!("unexpected resolution: {mismatch:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));
    assert_eq!(blob.mode, BlobMode::Regular);
    assert!(matches!(blob.content, BlobContent::Available { .. }));
}

#[test]
fn special_entries_are_never_followed() {
    let mut bed = bed();
    let (_i, sym) = bed
        .run(None, "docs/guide.md", false, "../alias")
        .unwrap_or_else(|_d| panic!());
    let Resolution::UnsupportedTarget(UnsupportedTarget::Symlink { path }) = sym else {
        panic!("unexpected resolution: {sym:?}");
    };
    assert_eq!(path.as_str(), Some("alias"));

    let (_i, gitlink) = bed
        .run(None, "docs/guide.md", false, "../module")
        .unwrap_or_else(|_d| panic!());
    let Resolution::UnsupportedTarget(UnsupportedTarget::Gitlink { path }) = gitlink else {
        panic!("unexpected resolution: {gitlink:?}");
    };
    assert_eq!(path.as_str(), Some("module"));
}

#[test]
fn empty_destinations_target_the_source_document() {
    let mut bed = bed();
    for destination in ["", "?q", "#"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {destination}: {row:?}");
        };
        assert_eq!(blob.path.as_str(), Some("docs/guide.md"));
    }

    let row = bed
        .run(None, "docs/guide.md", false, "#Intro")
        .unwrap_or_else(|_defect| panic!("resolve fragment"))
        .1;
    let Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let row = bed
        .run(None, "docs/guide.md", false, "#L1")
        .unwrap_or_else(|_defect| panic!("resolve line fragment"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let row = bed
        .run(None, "docs/guide.md", false, "#L2")
        .unwrap_or_else(|_defect| panic!("resolve out-of-range line fragment"))
        .1;
    let Resolution::Missing(Missing::LineFragmentOutOfRange { path }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/guide.md"));
}

#[test]
fn query_and_fragment_semantics_follow_the_precedence() {
    let mut bed = bed();
    for destination in [
        "data.json?x",
        "data.json?x#sym",
        "../vendor/inside.md?x",
        "../llms.txt?x",
    ] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::Query(_))
            ),
            "{destination}: {row:?}"
        );
    }

    let row = bed
        .run(None, "docs/guide.md", false, "guide.md?x#Intro")
        .unwrap_or_else(|_defect| panic!("resolve document fragment"))
        .1;
    assert!(matches!(
        row,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
    ));

    let row = bed
        .run(None, "docs/guide.md", false, "guide.md?x")
        .unwrap_or_else(|_defect| panic!("resolve ignored query"))
        .1;
    assert!(matches!(row, Resolution::Resolved(_)));

    let row = bed
        .run(None, "docs/guide.md", false, "data.json#anything")
        .unwrap_or_else(|_defect| panic!("resolve code fragment"))
        .1;
    assert!(matches!(
        row,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(_))
    ));

    let row = bed
        .run(None, "docs/guide.md", false, "guide.md#%zz")
        .unwrap_or_else(|_defect| panic!("resolve invalid fragment"))
        .1;
    assert_eq!(row, Resolution::Invalid(InvalidReference::FragmentEncoding));

    let (_i, retained) = bed
        .run(None, "docs/guide.md", false, "data.json?x")
        .unwrap_or_else(|_d| panic!());
    let Resolution::UnsupportedSemantics(UnsupportedSemantics::Query(Target::Blob(blob))) =
        retained
    else {
        panic!("unexpected resolution: {retained:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/data.json"));
    assert_eq!(blob.mode, BlobMode::Regular);
    assert!(matches!(blob.content, BlobContent::Available { .. }));
}

#[test]
fn line_fragments_have_a_hard_grammar() {
    let mut bed = bed();
    for destination in ["guide.md#L1", "guide.md#L1-L1"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Resolved(Target::Blob(_))),
            "{destination}: {row:?}"
        );
    }
    let row = bed
        .run(None, "docs/guide.md", false, "guide.md#L10-L20")
        .unwrap_or_else(|_defect| panic!("resolve out-of-range lines"))
        .1;
    assert!(matches!(
        row,
        Resolution::Missing(Missing::LineFragmentOutOfRange { .. })
    ));

    for renderer in ["L0", "l5", "L5-L2", "L", "L5x", "L05"] {
        let destination = format!("guide.md#{renderer}");
        let row = bed
            .run(None, "docs/guide.md", false, &destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
            ),
            "{renderer} is not the line grammar, and the target is a document: {row:?}"
        );
    }
}

fn expected_line_projection(mode: GitMode, selected: &[u8]) -> amiss_wire::digest::Digest {
    let selected_raw = hb(RAW_EVIDENCE_DOMAIN, selected);
    hj(
        TARGET_LINE_PROJECTION_DOMAIN,
        &Value::Object(vec![
            (
                "git_mode".to_owned(),
                Value::String(mode.as_str().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::String(selected_raw.to_string()),
            ),
        ]),
    )
}

#[test]
fn line_selections_digest_the_exact_raw_inclusive_slice() {
    let mut bed = bed();
    let selections: [(&str, &[u8]); 5] = [
        ("L1", b"one\r\n"),
        ("L2", b"two\n"),
        ("L3", b"three\r"),
        ("L4", b"four"),
        ("L2-L4", b"two\nthree\rfour"),
    ];

    for (fragment, selected) in selections {
        let row = bed
            .run(
                None,
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{fragment}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {fragment}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {fragment}: {row:?}");
        };
        let BlobContent::Available {
            raw_digest,
            projection_digest,
        } = blob.content
        else {
            panic!("unexpected content for {fragment}: {:?}", blob.content);
        };
        assert_eq!(
            raw_digest,
            hb(RAW_EVIDENCE_DOMAIN, MIXED_LINES),
            "the evidence digest remains the complete target for {fragment}"
        );
        assert_eq!(
            projection_digest,
            expected_line_projection(GitMode::RegularFile, selected),
            "the target projection is the exact selected bytes for {fragment}"
        );
    }

    let complete = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs")
        .unwrap_or_else(|_defect| panic!("resolve complete target"))
        .1;
    let all_lines = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L1-L4")
        .unwrap_or_else(|_defect| panic!("resolve all lines"))
        .1;
    let Resolution::Resolved(Target::Blob(complete)) = complete else {
        panic!("unexpected complete-target resolution: {complete:?}");
    };
    let Resolution::Resolved(Target::Blob(all_lines)) = all_lines else {
        panic!("unexpected all-lines resolution: {all_lines:?}");
    };
    assert_ne!(
        all_lines.content.projection_digest(),
        complete.content.projection_digest(),
        "a line selection stays domain-separated even when it spans the complete target"
    );
    assert_eq!(
        all_lines.content.projection_digest(),
        Some(expected_line_projection(GitMode::RegularFile, MIXED_LINES))
    );
}

#[test]
fn line_projection_ignores_bytes_outside_the_selected_slice() {
    let mut bed = bed();
    let original = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L2")
        .unwrap_or_else(|_defect| panic!("resolve original"))
        .1;
    let outside_changed = bed
        .run(
            None,
            "docs/guide.md",
            false,
            "../src/lines-outside-changed.rs#L2",
        )
        .unwrap_or_else(|_defect| panic!("resolve outside-changed"))
        .1;
    let Resolution::Resolved(Target::Blob(original)) = original else {
        panic!("unexpected original resolution: {original:?}");
    };
    let Resolution::Resolved(Target::Blob(outside_changed)) = outside_changed else {
        panic!("unexpected outside-changed resolution: {outside_changed:?}");
    };
    let BlobContent::Available {
        raw_digest: original_raw,
        projection_digest: original_projection,
    } = original.content
    else {
        panic!("unexpected original content: {:?}", original.content);
    };
    let BlobContent::Available {
        raw_digest: changed_raw,
        projection_digest: changed_projection,
    } = outside_changed.content
    else {
        panic!(
            "unexpected outside-changed content: {:?}",
            outside_changed.content
        );
    };
    assert_ne!(original_raw, changed_raw);
    assert_eq!(
        original_projection, changed_projection,
        "equal selected raw bytes stay equal when only bytes outside them differ"
    );
}

#[test]
fn executable_line_selections_bind_the_executable_mode() {
    let mut bed = bed();
    let row = bed
        .run(None, "docs/guide.md", false, "../src/executable.sh#L2")
        .unwrap_or_else(|_defect| panic!("resolve executable line"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected executable resolution: {row:?}");
    };
    assert_eq!(blob.mode, BlobMode::Executable);
    assert_eq!(
        blob.content.projection_digest(),
        Some(expected_line_projection(GitMode::ExecutableFile, b"two\n"))
    );
}

#[test]
fn line_selection_bounds_are_structural_missing_outcomes() {
    let mut bed = bed();
    for fragment in ["L5", "L4-L5", "L5-L5", "L9007199254740991"] {
        let row = bed
            .run(
                None,
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{fragment}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {fragment}"))
            .1;
        let Resolution::Missing(Missing::LineFragmentOutOfRange { path }) = row else {
            panic!("unexpected resolution for {fragment}: {row:?}");
        };
        assert_eq!(path.as_str(), Some("src/lines.rs"));
    }

    let empty = bed
        .run(None, "docs/guide.md", false, "../src/empty.rs#L1")
        .unwrap_or_else(|_defect| panic!("resolve empty target"))
        .1;
    assert!(matches!(
        empty,
        Resolution::Missing(Missing::LineFragmentOutOfRange { .. })
    ));

    for malformed in ["L0", "l2", "L", "L02", "L2-L1", "L2-3", "L9007199254740992"] {
        let row = bed
            .run(
                None,
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{malformed}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {malformed}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(_))
            ),
            "malformed line spelling {malformed} remains an unsupported code fragment: {row:?}"
        );
    }
}

#[test]
fn native_and_absolute_line_ranges_follow_the_declared_forge_dialect() {
    let mut bed = bed();
    let contexts = [github_context(), gitlab_context(), gitea_context()];
    let native_cases = [
        (&contexts[0], "L2-L3", "L2-3"),
        (&contexts[1], "L2-3", "L2-L3"),
        (&contexts[2], "L2-L3", "L2-3"),
    ];
    let expected = Some(expected_line_projection(
        GitMode::RegularFile,
        b"two\nthree\r",
    ));

    for (context, accepted, rejected) in native_cases {
        let row = bed
            .run(
                Some(context),
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{accepted}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {accepted}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {accepted}: {row:?}");
        };
        assert_eq!(blob.content.projection_digest(), expected, "{accepted}");

        let row = bed
            .run(
                Some(context),
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{rejected}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {rejected}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(_))
            ),
            "{rejected} is not the declared dialect's range spelling: {row:?}"
        );

        let out_of_range = bed
            .run(Some(context), "docs/guide.md", false, "../src/lines.rs#L5")
            .unwrap_or_else(|_defect| panic!("resolve out of range"))
            .1;
        assert!(matches!(
            out_of_range,
            Resolution::Missing(Missing::LineFragmentOutOfRange { .. })
        ));
    }

    let absolute_cases = [
        (
            &contexts[0],
            "https://github.com/acme/widgets/blob/feature/x/src/lines.rs#L2-L3",
        ),
        (
            &contexts[1],
            "https://gitlab.com/acme/widgets/-/blob/feature/x/src/lines.rs#L2-3",
        ),
        (
            &contexts[2],
            "https://codeberg.org/acme/widgets/src/branch/feature/x/src/lines.rs#L2-L3",
        ),
    ];
    for (context, destination) in absolute_cases {
        let row = bed
            .run(Some(context), "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {destination}: {row:?}");
        };
        assert_eq!(blob.content.projection_digest(), expected, "{destination}");
    }
}

#[test]
fn lfs_pointer_targets_resolve_with_pointer_availability() {
    let mut bed = bed();
    let (_i, row) = bed
        .run(None, "docs/guide.md", false, "../pointer.bin")
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("pointer.bin"));
    assert_eq!(blob.mode, BlobMode::Regular);
    let BlobContent::LfsPointer { raw_digest } = blob.content else {
        panic!("unexpected blob content: {:?}", blob.content);
    };
    assert_eq!(raw_digest, hb(RAW_EVIDENCE_DOMAIN, POINTER.as_bytes()));

    let selected = bed
        .run(None, "docs/guide.md", false, "../pointer.bin#L1")
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
        .run(None, "docs/guide.md", false, "data.json")
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved(Target::Blob(blob)) = row else {
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
        &Value::Object(vec![
            ("git_mode".to_owned(), Value::String("100644".to_owned())),
            ("raw_digest".to_owned(), Value::String(raw.to_string())),
        ]),
    );
    assert_eq!(projection_digest, projection);
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
fn a_reused_target_cache_tracks_object_and_scan_scope() {
    let mut bed = bed();
    let first = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L2")
        .unwrap_or_else(|_defect| panic!("resolve first snapshot"))
        .1;
    let Resolution::Resolved(Target::Blob(first)) = first else {
        panic!("unexpected first resolution: {first:?}");
    };

    let changed = b"one\r\nchanged\nthree\rfour";
    fs::write(bed.dir.path().join("src/lines.rs"), changed)
        .unwrap_or_else(|_defect| panic!("write changed target"));
    git(bed.dir.path(), &["add", "src/lines.rs"]);
    git(bed.dir.path(), &["commit", "-qm", "change target"]);
    let tree = Oid::new(
        ObjectFormat::Sha1,
        git(bed.dir.path(), &["rev-parse", "HEAD^{tree}"])
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
        .run(None, "docs/guide.md", false, "../src/lines.rs#L2")
        .unwrap_or_else(|_defect| panic!("resolve changed snapshot"))
        .1;
    let Resolution::Resolved(Target::Blob(second)) = second else {
        panic!("unexpected changed resolution: {second:?}");
    };
    assert_ne!(
        first.content, second.content,
        "a new object at one path cannot reuse stale body or line evidence"
    );

    bed.scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let repeated = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L2")
        .unwrap_or_else(|_defect| panic!("resolve in fresh scan scope"))
        .1;
    assert_eq!(repeated, Resolution::Resolved(Target::Blob(second)));
    let changed_len = u64::try_from(changed.len()).unwrap_or(u64::MAX);
    assert_eq!(bed.scan_resources.target_bytes(), changed_len);
    assert_eq!(bed.scan_resources.line_fragment_bytes(), changed_len);
}

#[test]
fn distinct_line_selections_are_bounded_and_cached() {
    let target_bytes = u64::try_from(MIXED_LINES.len()).unwrap_or(u64::MAX);
    let mut bed = bed_with(ScanLimits {
        aggregate_line_fragment_evaluation_bytes_per_snapshot: target_bytes,
        ..ScanLimits::CONTRACT
    });

    assert!(
        bed.run(None, "docs/guide.md", false, "../src/lines.rs#L2")
            .is_ok()
    );
    assert_eq!(bed.scan_resources.line_fragment_bytes(), target_bytes);
    assert!(
        bed.run(None, "docs/guide.md", false, "../src/lines.rs#L2")
            .is_ok()
    );
    assert_eq!(
        bed.scan_resources.line_fragment_bytes(),
        target_bytes,
        "an identical selection reuses its cached projection"
    );

    let crossing = bed.run(None, "docs/guide.md", false, "../src/lines.rs#L3");
    assert_eq!(
        crossing,
        Err(Error::ResourceLimit {
            resource: ResourceName::AggregateLineFragmentEvaluationBytesPerSnapshot,
            configured_limit: target_bytes,
            observed_lower_bound: target_bytes.saturating_mul(2),
        })
    );

    let mut missing_bed = bed_with(ScanLimits {
        aggregate_line_fragment_evaluation_bytes_per_snapshot: target_bytes,
        ..ScanLimits::CONTRACT
    });
    let first_missing = missing_bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L5")
        .unwrap_or_else(|_defect| panic!("resolve first out-of-range selection"));
    let repeated_missing = missing_bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L5")
        .unwrap_or_else(|_defect| panic!("resolve cached out-of-range selection"));
    assert_eq!(first_missing, repeated_missing);
    assert_eq!(
        missing_bed.scan_resources.line_fragment_bytes(),
        target_bytes,
        "an out-of-range selection caches its absence as well as its charge"
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
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/ACME/widgets/blob/main/docs/guide.md",
        )
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGithub);
    let Resolution::UnsupportedVersion(VersionScope::KnownPath { path }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/guide.md"));
}

#[test]
fn github_with_a_different_trusted_identity_is_foreign() {
    let mut bed = bed();
    let context = github_context();
    let (intent, foreign) = bed
        .run(
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
        .run(
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
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/main/../x",
        )
        .unwrap_or_else(|_d| panic!())
        .1;
    assert_eq!(row, Resolution::Invalid(InvalidReference::PathTraversal));

    let row = bed
        .run(
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
        .run(
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
    let context = github_context();
    let (_i, tree) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/tree/feature/x/docs/",
        )
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved(Target::Tree { path }) = tree else {
        panic!("unexpected resolution: {tree:?}");
    };
    assert_eq!(path.as_str(), Some("docs"));

    let (_i, lines) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/src/lib.rs#L1-L1",
        )
        .unwrap_or_else(|_d| panic!());
    let Resolution::Resolved(Target::Blob(blob)) = lines else {
        panic!("unexpected resolution: {lines:?}");
    };
    assert_eq!(blob.path.as_str(), Some("src/lib.rs"));
    assert!(matches!(blob.content, BlobContent::Available { .. }));

    let (_i, tree_fragment) = bed
        .run(
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
    assert_eq!(
        row,
        Resolution::UnsupportedVersion(VersionScope::UnknownPath)
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

    for destination in ["guide.md", "../README"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Resolved(_)),
            "{destination}: {row:?}"
        );
    }
    for destination in ["Guide.md", "GUIDE.MD", "../readme"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Missing(Missing::PathNotFound { .. })),
            "{destination}: {row:?}"
        );
    }

    // U+00E9, the precomposed accent the tree actually carries.
    let row = bed
        .run(None, "docs/guide.md", false, "\u{e9}t\u{e9}.txt")
        .unwrap_or_else(|_defect| panic!("resolve precomposed path"))
        .1;
    assert!(matches!(row, Resolution::Resolved(_)));
    // The same two accents decomposed into e + U+0301: the same text, other bytes.
    let row = bed
        .run(None, "docs/guide.md", false, "e\u{301}te\u{301}.txt")
        .unwrap_or_else(|_defect| panic!("resolve decomposed path"))
        .1;
    assert!(matches!(
        row,
        Resolution::Missing(Missing::PathNotFound { .. })
    ));
}

#[test]
fn forge_urls_without_a_declared_context_are_external() {
    let mut bed = bed();
    let urls = [
        "https://github.com/acme/widgets/blob/feature/x/docs/guide.md",
        "https://gitlab.com/acme/widgets/-/blob/feature/x/docs/guide.md",
        "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/guide.md",
    ];

    for url in urls {
        let (intent, row) = bed
            .run(None, "docs/guide.md", false, url)
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
    let context = github_context();
    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://github.com.evil.example/acme/widgets/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(row, Resolution::External(ExternalReference::Url));
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
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let (_intent, encoded) = bed
        .run(
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
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/blob/0123456789012345678901234567890123456789/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        pinned,
        Resolution::UnsupportedVersion(VersionScope::UnknownPath),
        "a commit-pinned link matches neither trusted ref, exactly as on github"
    );
}

fn gitea_context() -> ForgeContext {
    ForgeContext {
        host: "codeberg.org".to_owned(),
        dialect: ForgeDialect::Gitea,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/feature/x".to_owned(),
        default_ref: "refs/heads/main".to_owned(),
        candidate_oid: Some("6a66ef14b9b8b174a54ccf8ea4b0dd18f42f9f22".to_owned()),
    }
}

/// The gitea family against a real tree: the typed branch form resolves
/// with an either target, the commit form is pinned to the exact candidate
/// OID, a tag spelled like the candidate branch stays version-scoped out,
/// and the untyped legacy form is foreign.
#[test]
fn gitea_recognition_resolves_against_the_tree() {
    let mut bed = bed();
    let context = gitea_context();
    let (intent, row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGitea);
    assert_eq!(intent.target_kind, Some(TargetKind::Either));
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let (_intent, pinned) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/commit/6a66ef14b9b8b174a54ccf8ea4b0dd18f42f9f22/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert!(
        matches!(pinned, Resolution::Resolved(_)),
        "the candidate commit's own OID resolves in the candidate"
    );

    let (_intent, tag) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/tag/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        tag,
        Resolution::UnsupportedVersion(VersionScope::UnknownPath),
        "a tag spelled like the candidate branch is still no trusted ref"
    );

    let (_intent, untyped) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        untyped,
        Resolution::External(ExternalReference::ForeignRepository)
    );

    let (_intent, index_mode) = bed
        .run(
            Some(&ForgeContext {
                candidate_oid: None,
                ..gitea_context()
            }),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/commit/6a66ef14b9b8b174a54ccf8ea4b0dd18f42f9f22/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    let Resolution::UnsupportedVersion(VersionScope::KnownPath { path }) = index_mode else {
        panic!("unexpected resolution: {index_mode:?}");
    };
    assert_eq!(
        path.as_str(),
        Some("docs/guide.md"),
        "with no candidate commit no OID can match, path disclosed"
    );
}
