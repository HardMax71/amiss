use amiss_scan::Resolution;
use amiss_scan::resolve::ForgeContext;
use amiss_wire::controls::TargetKind;
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{ExternalReference, Target, VersionScope};

use crate::support::{bed, gitea_context, github_context, gitlab_context};

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

/// One wrong fact makes a same-repository spelling foreign: the owner, the
/// project, or the form, each alone, on every dialect.
#[test]
fn one_wrong_fact_makes_a_foreign_url() {
    let mut bed = bed();
    let cases = [
        (
            github_context(),
            "https://github.com/other/widgets/blob/main/docs/guide.md",
        ),
        (
            github_context(),
            "https://github.com/acme/other/blob/main/docs/guide.md",
        ),
        (
            github_context(),
            "https://github.com/acme/widgets/raw/main/docs/guide.md",
        ),
        (
            gitea_context(),
            "https://codeberg.org/other/widgets/src/branch/main/docs/guide.md",
        ),
        (
            gitea_context(),
            "https://codeberg.org/acme/other/src/branch/main/docs/guide.md",
        ),
        (
            gitlab_context(),
            "https://gitlab.com/group/widgets/-/blob/main/docs/guide.md",
        ),
    ];
    for (context, destination) in cases {
        let (intent, _row) = bed
            .run(Some(&context), "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!());
        assert_eq!(intent.kind, IntentKind::ExternalUrl, "{destination}");
    }
}

/// A commit selector is a full lowercase object id and nothing looser.
#[test]
fn a_commit_selector_is_an_exact_oid() {
    let mut bed = bed();
    let context = gitea_context();
    for selector in [
        "6A66EF14B9B8B174A54CCF8EA4B0DD18F42F9F22",
        "6a66ef14b9b8b174a54ccf8ea4b0dd18f42f9f2",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
    ] {
        let (intent, _row) = bed
            .run(
                Some(&context),
                "docs/guide.md",
                false,
                &format!("https://codeberg.org/acme/widgets/src/commit/{selector}/docs/guide.md"),
            )
            .unwrap_or_else(|_defect| panic!());
        assert_eq!(
            intent.kind,
            IntentKind::ExternalUrl,
            "{selector} is not a commit spelling the forge emits"
        );
    }
}

/// A nested group owner matches segment by segment, and the separator may sit
/// past position two exactly when the owner has that many segments.
#[test]
fn nested_group_owners_match_segment_by_segment() {
    let mut bed = bed();
    let nested = ForgeContext {
        owner: "group/sub".to_owned(),
        ..gitlab_context()
    };
    let (intent, row) = bed
        .run(
            Some(&nested),
            "docs/guide.md",
            false,
            "https://gitlab.com/group/sub/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(intent.kind, IntentKind::SameRepositoryGitlab);
    assert!(matches!(row, Resolution::Resolved(_)));

    let (foreign, _row) = bed
        .run(
            Some(&nested),
            "docs/guide.md",
            false,
            "https://gitlab.com/group/other/widgets/-/blob/feature/x/docs/guide.md",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        foreign.kind,
        IntentKind::ExternalUrl,
        "one wrong owner segment is foreign"
    );
}

/// A terminal slash is a directory hint exactly where the form tolerates it.
#[test]
fn a_terminal_slash_is_tolerated_only_as_a_directory_hint() {
    let mut bed = bed();
    let context = gitlab_context();
    let (tree, tree_row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/tree/feature/x/docs/",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(tree.kind, IntentKind::SameRepositoryGitlab);
    assert!(
        matches!(tree_row, Resolution::Resolved(_)),
        "a tree with a directory-hint slash resolves: {tree_row:?}"
    );

    let (_blob, blob_row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://gitlab.com/acme/widgets/-/blob/feature/x/docs/guide.md/",
        )
        .unwrap_or_else(|_defect| panic!());
    assert!(
        !matches!(blob_row, Resolution::Resolved(_)),
        "a blob does not tolerate the hint: {blob_row:?}"
    );
}

/// The gitea directory hint needs at least one real segment before the
/// terminal slash, and a bare slash after the ref is a syntax defect.
#[test]
fn a_directory_hint_needs_a_segment_before_its_slash() {
    let mut bed = bed();
    let context = gitea_context();
    let (hinted, hinted_row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(hinted.kind, IntentKind::SameRepositoryGitea);
    assert_eq!(hinted.target_kind, Some(TargetKind::Tree));
    assert!(
        matches!(hinted_row, Resolution::Resolved(_)),
        "{hinted_row:?}"
    );

    let (bare, bare_row) = bed
        .run(
            Some(&context),
            "docs/guide.md",
            false,
            "https://codeberg.org/acme/widgets/src/branch/feature/x/",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(bare.target_kind, None, "no hint from a bare slash");
    assert!(
        !matches!(bare_row, Resolution::Resolved(_)),
        "a bare slash after the ref is not a target: {bare_row:?}"
    );

    let (github, github_row) = bed
        .run(
            Some(&github_context()),
            "docs/guide.md",
            false,
            "https://github.com/acme/widgets/tree/feature/x/",
        )
        .unwrap_or_else(|_defect| panic!());
    assert_eq!(
        github.kind,
        IntentKind::Unsupported,
        "a lone terminal slash after the ref is no spelling of a target"
    );
    assert!(
        !matches!(github_row, Resolution::Resolved(_)),
        "a lone terminal slash after the ref stays a defect: {github_row:?}"
    );
}
