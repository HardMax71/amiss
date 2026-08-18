use amiss_scan::Resolution;
use amiss_wire::controls::TargetKind;
use amiss_wire::model::Adapter;
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    BlobContent, BlobMode, ExternalReference, InvalidReference, Missing, Target,
    UnsupportedSemantics, UnsupportedTarget,
};

use crate::support::bed;

#[test]
fn component_splitting_follows_rfc_order() {
    let mut bed = bed();
    let (intent, row) = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "https://e.com/a?x?y#z?u",
        )
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
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
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
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Invalid(InvalidReference::Uri)),
            "{destination}: {row:?}"
        );
    }

    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "//cdn.e.com/x",
        )
        .unwrap_or_else(|_defect| panic!("resolve network path"))
        .1;
    assert!(matches!(
        row,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::NetworkPath)
    ));

    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "/guide/start",
        )
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
        ("a%2Fb%zz", InvalidReference::PercentEncoding),
        ("a%00b.md", InvalidReference::DecodedPathControl),
        ("a//b.md", InvalidReference::Syntax),
        ("sub//", InvalidReference::Syntax),
    ] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert_eq!(row, Resolution::Invalid(reason), "{destination}");
    }
    for destination in ["guide.md", "./guide.md", "%2E%2E/README"] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Resolved(_)),
            "{destination}: {row:?}"
        );
    }
    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/deep/source.md",
            false,
            "../../src/./l%69b.rs",
        )
        .unwrap_or_else(|_defect| panic!("resolve a normalized deep path"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("src/lib.rs"));

    let row = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "absent.md")
        .unwrap_or_else(|_defect| panic!("resolve absent path"))
        .1;
    let Resolution::Missing(Missing::PathNotFound { path, .. }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/absent.md"));

    // `%25` decodes to a literal `%` and stops there. A second pass is what turns
    // `%252E%252E/` into `../` and `%252F` into a separator, so the whole defence
    // is that the pass never happens: each of these is a filename with per cent
    // signs in it, and none of them is a path.
    for destination in ["%252E%252E/README", "docs%252Fguide.md", "a%252Fb.md"] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
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
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "sub/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(intent.target_kind, Some(TargetKind::Tree));
    let Resolution::Resolved(Target::Tree { path }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/sub"));

    let (_intent, image) = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", true, "sub/")
        .unwrap_or_else(|_d| panic!());
    assert_eq!(image, Resolution::Invalid(InvalidReference::Syntax));

    let (intent, mismatch) = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "guide.md/")
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
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "../alias")
        .unwrap_or_else(|_d| panic!());
    let Resolution::UnsupportedTarget(UnsupportedTarget::Symlink { path }) = sym else {
        panic!("unexpected resolution: {sym:?}");
    };
    assert_eq!(path.as_str(), Some("alias"));

    let (_i, gitlink) = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "../module")
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
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {destination}: {row:?}");
        };
        assert_eq!(blob.path.as_str(), Some("docs/guide.md"));
    }

    let row = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "#guide")
        .unwrap_or_else(|_defect| panic!("resolve self anchor"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let row = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "#Intro")
        .unwrap_or_else(|_defect| panic!("resolve absent anchor"))
        .1;
    let Resolution::Missing(Missing::HeadingAnchorNotFound { path, .. }) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/guide.md"));

    let row = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "#L1")
        .unwrap_or_else(|_defect| panic!("resolve line fragment"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected resolution: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));

    let row = bed
        .run_as(Adapter::Markdown, None, "docs/guide.md", false, "#L2")
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
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
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
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "guide.md?x#Intro",
        )
        .unwrap_or_else(|_defect| panic!("resolve document fragment"))
        .1;
    assert!(matches!(
        row,
        Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
    ));

    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "guide.md?x",
        )
        .unwrap_or_else(|_defect| panic!("resolve ignored query"))
        .1;
    assert!(matches!(row, Resolution::Resolved(_)));

    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "data.json#anything",
        )
        .unwrap_or_else(|_defect| panic!("resolve code fragment"))
        .1;
    assert!(matches!(
        row,
        Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(_))
    ));

    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "guide.md#%zz",
        )
        .unwrap_or_else(|_defect| panic!("resolve invalid fragment"))
        .1;
    assert_eq!(row, Resolution::Invalid(InvalidReference::FragmentEncoding));

    let (_i, retained) = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "data.json?x",
        )
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
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Resolved(_)),
            "{destination}: {row:?}"
        );
    }
    for destination in ["Guide.md", "GUIDE.MD", "../readme"] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Missing(Missing::PathNotFound { .. })),
            "{destination}: {row:?}"
        );
    }

    // U+00E9, the precomposed accent the tree actually carries.
    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "\u{e9}t\u{e9}.txt",
        )
        .unwrap_or_else(|_defect| panic!("resolve precomposed path"))
        .1;
    assert!(matches!(row, Resolution::Resolved(_)));
    // The same two accents decomposed into e + U+0301: the same text, other bytes.
    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "e\u{301}te\u{301}.txt",
        )
        .unwrap_or_else(|_defect| panic!("resolve decomposed path"))
        .1;
    assert!(matches!(
        row,
        Resolution::Missing(Missing::PathNotFound { .. })
    ));
}

#[test]
fn a_lowercase_hex_escape_decodes_like_its_uppercase_twin() {
    let mut bed = bed();
    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/index.md",
            false,
            "guide.%6dd",
        )
        .unwrap_or_else(|_defect| panic!("resolve lowercase escape"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = &row else {
        panic!("%6d spells m in either case: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));
}

#[test]
fn an_authority_is_judged_by_its_exact_grammar() {
    let mut bed = bed();
    for accepted in [
        "https://example.com/x",
        "https://[::1]/x",
        "https://[::1]:8080/x",
    ] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/index.md", false, accepted)
            .unwrap_or_else(|_defect| panic!("resolve {accepted}"))
            .1;
        assert!(
            matches!(row, Resolution::External(_)),
            "{accepted}: {row:?}"
        );
    }
    for refused in [
        "https://ex\u{e4}mple.com/x",
        "https://[]:8080/x",
        "https://a[b/x",
    ] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/index.md", false, refused)
            .unwrap_or_else(|_defect| panic!("resolve {refused}"))
            .1;
        assert!(
            matches!(row, Resolution::Invalid(InvalidReference::Uri)),
            "{refused}: {row:?}"
        );
    }
}
