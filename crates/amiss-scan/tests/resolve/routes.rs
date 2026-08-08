use amiss_scan::Resolution;
use amiss_wire::model::RepoPath;
use amiss_wire::resolution::{Missing, Target};

use amiss_wire::model::Adapter;
use amiss_wire::resolution::UnsupportedSemantics;

use crate::support::{bed, github_context};

/// A destination the tree does not hold is answered by the file a modelled
/// router would serve under that spelling, and the report names that file.
#[test]
fn a_router_spelling_reaches_the_source_file_it_serves() {
    let mut bed = bed();
    for (destination, served) in [
        ("guide", "docs/guide.md"),
        ("guide.html", "docs/guide.md"),
        ("sub/index.md", "docs/sub/README.md"),
        ("sub/index.html", "docs/sub/README.md"),
    ] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/index.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = &row else {
            panic!("{destination} is served by a known router: {row:?}");
        };
        assert_eq!(blob.path.as_str(), Some(served), "{destination}");
    }
}

/// The written destination stays the reference's identity, so the intent keeps
/// the spelling the author used while the resolution names what answered it.
#[test]
fn a_routed_reference_keeps_the_authored_destination_as_its_intent() {
    let mut bed = bed();
    let (intent, row) = bed
        .run_as(Adapter::Markdown, None, "docs/index.md", false, "guide")
        .unwrap_or_else(|_defect| panic!("resolve guide"));
    assert_eq!(
        intent.repository_path.as_ref().and_then(RepoPath::as_str),
        Some("docs/guide")
    );
    let Resolution::Resolved(Target::Blob(blob)) = &row else {
        panic!("guide resolves: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));
}

/// A spelling only ever names a file the tree already holds, so nothing it
/// reaches was invented and everything else stays missing.
#[test]
fn a_spelling_never_invents_a_target() {
    let mut bed = bed();
    for destination in ["absent", "absent.html", "sub/absent/index.md", "data"] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/index.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Missing(Missing::PathNotFound { path, .. }) = &row else {
            panic!("{destination} names no file any router serves: {row:?}");
        };
        assert_eq!(
            path.as_str(),
            Some(format!("docs/{destination}").as_str()),
            "the missing row names what the author wrote"
        );
    }
}

/// A trailing slash promises a directory, and a forge URL is read by the forge
/// rather than by a site router, so neither is ever re-spelled.
#[test]
fn a_promised_directory_and_a_forge_url_are_never_re_spelled() {
    let mut bed = bed();
    let row = bed
        .run_as(Adapter::Markdown, None, "docs/index.md", false, "guide/")
        .unwrap_or_else(|_defect| panic!("resolve guide/"))
        .1;
    let Resolution::Missing(Missing::PathNotFound { path, .. }) = &row else {
        panic!("a promised directory stays missing: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/guide"));

    let context = github_context();
    let row = bed
        .run_as(
            Adapter::Markdown,
            Some(&context),
            "docs/index.md",
            false,
            "https://github.com/acme/widgets/blob/feature/x/docs/guide",
        )
        .unwrap_or_else(|_defect| panic!("resolve the forge URL"))
        .1;
    let Resolution::Missing(Missing::PathNotFound { path, .. }) = &row else {
        panic!("a forge URL keeps the tree's answer: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/guide"));
}

/// The fragment is read against the file that answered, so an anchor on a
/// routed reference is evaluated rather than declined.
#[test]
fn a_fragment_on_a_routed_reference_reads_the_served_file() {
    let mut bed = bed();
    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/index.md",
            false,
            "anchors#setup--config",
        )
        .unwrap_or_else(|_defect| panic!("resolve the routed anchor"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = &row else {
        panic!("the anchor resolves on the served file: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/anchors.md"));

    let row = bed
        .run_as(
            Adapter::Markdown,
            None,
            "docs/index.md",
            false,
            "anchors#absent-heading",
        )
        .unwrap_or_else(|_defect| panic!("resolve the routed anchor"))
        .1;
    let Resolution::Missing(Missing::HeadingAnchorNotFound { path, .. }) = &row else {
        panic!("an absent identity is missing on the served file: {row:?}");
    };
    assert_eq!(path.as_str(), Some("docs/anchors.md"));
}

#[test]
fn an_asciidoc_page_identity_is_a_catalogue_question() {
    let mut bed = bed();
    let row = bed
        .run_as(Adapter::AsciiDoc, None, "docs/index.md", false, "guide")
        .unwrap_or_else(|_defect| panic!("resolve adoc page identity"))
        .1;
    assert!(
        matches!(
            row,
            Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent)
        ),
        "{row:?}"
    );
    let row = bed
        .run_as(Adapter::AsciiDoc, None, "docs/index.md", false, "guide.md")
        .unwrap_or_else(|_defect| panic!("resolve adoc file path"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = &row else {
        panic!("a dotted segment is a file, not a page identity: {row:?}");
    };
    assert_eq!(blob.path.as_str(), Some("docs/guide.md"));
}

#[test]
fn an_attribute_reference_waits_for_the_build_and_empty_braces_do_not() {
    let mut bed = bed();
    for waiting in ["x{imagesdir}y.md", "x{a-b}y.md", "x{a_b}y.md"] {
        let row = bed
            .run_as(Adapter::AsciiDoc, None, "docs/index.md", false, waiting)
            .unwrap_or_else(|_defect| panic!("resolve {waiting}"))
            .1;
        assert!(
            matches!(
                row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent)
            ),
            "{waiting}: {row:?}"
        );
    }
    let row = bed
        .run_as(Adapter::AsciiDoc, None, "docs/index.md", false, "a{}b.md")
        .unwrap_or_else(|_defect| panic!("resolve empty braces"))
        .1;
    assert!(
        matches!(row, Resolution::Missing(Missing::PathNotFound { .. })),
        "empty braces name no attribute: {row:?}"
    );
}
