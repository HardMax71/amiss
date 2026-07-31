use amiss_scan::Resolution;
use amiss_scan::resolve::ForgeContext;
use amiss_wire::controls::TargetKind;
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{ExternalReference, Target, VersionScope};

use crate::support::{bed, gitea_context};

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
