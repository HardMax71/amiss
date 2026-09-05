use amiss_fixtures::{CommitChain, Staged, staged_repository};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::resolve::{Resolver, TargetCache};
use amiss_scan::{Resolution, ScanLimits, ScanResources, discover};
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::resolution::{Missing, Target, UnsupportedSemantics};

use crate::support::{ANCHORS, bed, bed_with};

/// The identity of a heading belongs to the renderer, so an anchor resolves
/// when any pinned renderer would publish it. Nothing a repository declares can
/// narrow that set.
#[test]
fn a_heading_anchor_resolves_under_the_union_of_the_renderer_rules() {
    let mut bed = bed();
    for fragment in [
        "setup--config",
        "setup-config",
        "setup--config-1",
        "setup-config_1",
        "r%C3%A9sum%C3%A9-draft",
        "resume-draft",
        "declared",
        "custom",
        "explicit-custom",
        "tool",
        "anchor-point",
        "pair-id",
    ] {
        let destination = format!("anchors.md#{fragment}");
        let row = bed
            .run_as(
                Adapter::Markdown,
                None,
                "docs/guide.md",
                false,
                &destination,
            )
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Resolved {
            target: Target::Blob(blob),
        } = &row
        else {
            panic!("{fragment} is published by a known renderer: {row:?}");
        };
        assert_eq!(blob.path.as_str(), Some("docs/anchors.md"));
    }

    for fragment in ["Setup--Config", "setup", "résumé", "customid"] {
        let destination = format!("anchors.md#{fragment}");
        let row = bed
            .run_as(
                Adapter::Markdown,
                None,
                "docs/guide.md",
                false,
                &destination,
            )
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Missing(Missing::HeadingAnchorNotFound { path, .. }) = &row else {
            panic!("{fragment} is published by no renderer: {row:?}");
        };
        assert_eq!(path.as_str(), Some("docs/anchors.md"));
    }
}

/// A target the evaluation cannot read, parse, or afford keeps the unsupported
/// answer. Reporting it missing would be reporting on a parse that never ran.
#[test]
fn an_unevaluable_anchor_target_stays_unsupported_semantics() {
    let mut bed = bed();
    for destination in ["pointer.md#any", "invalid.md#any", "../llms.txt#any"] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
            ),
            "{destination}: {row:?}"
        );
    }

    let mut starved = bed_with(ScanLimits {
        aggregate_heading_anchor_evaluation_bytes_per_snapshot: 0,
        ..ScanLimits::CONTRACT
    });
    let row = starved
        .run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            "anchors.md#setup--config",
        )
        .unwrap_or_else(|_defect| panic!("resolve under an exhausted anchor budget"))
        .1;
    assert!(
        matches!(
            &row,
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
        ),
        "an exhausted budget judges nothing: {row:?}"
    );
}

/// The identities are built once per target, so repeated anchors into one
/// document are charged once.
#[test]
fn distinct_anchors_into_one_target_are_charged_once() {
    let mut bed = bed();
    for fragment in ["setup--config", "resume-draft", "declared"] {
        let destination = format!("anchors.md#{fragment}");
        bed.run_as(
            Adapter::Markdown,
            None,
            "docs/guide.md",
            false,
            &destination,
        )
        .unwrap_or_else(|_defect| panic!("resolve {destination}"));
    }
    assert_eq!(
        bed.scan_resources.heading_anchor_bytes(),
        u64::try_from(ANCHORS.len()).unwrap_or(u64::MAX),
        "one charge for the one target"
    );
}

/// One frozen tree covering exact, recursive, literal, refused, cyclic, and
/// unavailable include edges.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn transclusion_fixture() -> CommitChain {
    staged_repository(&[
        (
            "README.md",
            Staged::File(
                b"[a](docs/host.rst#present)\n[b](docs/host.rst#spliced)\n[c](docs/host.rst#absent)\n",
            ),
        ),
        (
            "docs/host.rst",
            Staged::File(b"Present\n=======\n\n.. include:: part.rst\n"),
        ),
        (
            "docs/part.rst",
            Staged::File(b"Spliced\n=======\n\n.. INCLUDE:: sub part.rst\n"),
        ),
        ("docs/sub part.rst", Staged::File(b"Hidden\n======\n")),
        (
            "docs/host.adoc",
            Staged::File(b"= Repeat\n\ninclude::parts/first.adoc[]\n\n== Repeat 2\n"),
        ),
        (
            "docs/parts/first.adoc",
            Staged::File(b"== Repeat\n\ninclude::nested/second.adoc[]\n"),
        ),
        (
            "docs/parts/nested/second.adoc",
            Staged::File(b"=== Deep\n"),
        ),
        (
            "docs/options.adoc",
            Staged::File(b"= Known\n\ninclude::parts/first.adoc[tags=first]\n"),
        ),
        (
            "docs/cycle-a.rst",
            Staged::File(b"A\n===\n\n.. include:: cycle-b.rst\n"),
        ),
        (
            "docs/cycle-b.rst",
            Staged::File(b"B\n===\n\n.. include:: cycle-a.rst\n"),
        ),
        (
            "docs/literal.rst",
            Staged::File(b"Literal\n=======\n\n.. literalinclude:: example.py\n"),
        ),
        (
            "docs/literal-missing.rst",
            Staged::File(b"Literal\n=======\n\n.. literalinclude:: absent.py\n"),
        ),
        ("docs/example.py", Staged::File(b"Not\n===\n")),
    ])
    .unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn transcluded(resolver: &mut Resolver<'_>, destination: &str) -> Resolution {
    resolver
        .resolve(
            None,
            Adapter::Markdown,
            &RepoPath::new("README.md".to_owned()).unwrap(),
            false,
            destination,
        )
        .unwrap()
        .1
}

fn assert_transclusion_matrix(resolver: &mut Resolver<'_>) {
    let held = transcluded(resolver, "docs/host.rst#present");
    assert!(matches!(held, Resolution::Resolved { .. }), "{held:?}");

    let spliced = transcluded(resolver, "docs/host.rst#spliced");
    assert!(
        matches!(spliced, Resolution::Resolved { .. }),
        "{spliced:?}"
    );
    let whitespace = transcluded(resolver, "docs/host.rst#hidden");
    assert!(
        matches!(whitespace, Resolution::Resolved { .. }),
        "directive names ignore case and their final path accepts whitespace: {whitespace:?}"
    );

    let nested = transcluded(resolver, "docs/host.adoc#_deep");
    assert!(
        matches!(nested, Resolution::Resolved { .. }),
        "nested paths are relative to the including file: {nested:?}"
    );
    let ordered = transcluded(resolver, "docs/host.adoc#_repeat_2_2");
    assert!(
        matches!(ordered, Resolution::Resolved { .. }),
        "included headings occupy identities at the directive position: {ordered:?}"
    );
    let asciidoc_absent = transcluded(resolver, "docs/host.adoc#_absent");
    assert!(
        matches!(asciidoc_absent, Resolution::UnsupportedSemantics(_)),
        "unmodelled AsciiDoc attribute state keeps absence undecided: {asciidoc_absent:?}"
    );

    let absent = transcluded(resolver, "docs/host.rst#absent");
    assert!(
        matches!(
            absent,
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        ),
        "a complete expanded anchor set can prove absence: {absent:?}"
    );

    let selected = transcluded(resolver, "docs/options.adoc#_repeat");
    assert!(
        matches!(selected, Resolution::UnsupportedSemantics(_)),
        "an unsupported selector cannot guess which headings were included: {selected:?}"
    );

    let before_cycle = transcluded(resolver, "docs/cycle-a.rst#b");
    assert!(
        matches!(before_cycle, Resolution::Resolved { .. }),
        "known identities before a cycle remain evidence: {before_cycle:?}"
    );
    let beyond_cycle = transcluded(resolver, "docs/cycle-a.rst#absent");
    assert!(
        matches!(beyond_cycle, Resolution::UnsupportedSemantics(_)),
        "a cycle leaves absence undecided: {beyond_cycle:?}"
    );

    let literal_absent = transcluded(resolver, "docs/literal.rst#absent");
    assert!(
        matches!(
            literal_absent,
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        ),
        "literal code contributes no parsed headings: {literal_absent:?}"
    );
    let unavailable_literal = transcluded(resolver, "docs/literal-missing.rst#absent");
    assert!(
        matches!(unavailable_literal, Resolution::UnsupportedSemantics(_)),
        "an unavailable literal target keeps the document partial: {unavailable_literal:?}"
    );
}

#[test]
fn bounded_local_includes_publish_only_proven_heading_anchors() {
    let dir = transclusion_fixture();
    let tree = Oid::new(
        ObjectFormat::Sha1,
        dir.commits.first().unwrap().tree.clone(),
    )
    .unwrap();
    let repo = Repository::open(dir.root(), ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let includes = amiss_scan::Includes::default();
    let discovery = discover(
        &repo,
        &mut git_resources,
        &mut scan_resources,
        &includes,
        &tree,
    )
    .unwrap();
    {
        let mut cache = TargetCache::default();
        let mut resolver = Resolver::new(
            &repo,
            &mut git_resources,
            &mut scan_resources,
            &mut cache,
            &discovery,
        );
        assert_transclusion_matrix(&mut resolver);
    }

    let mut limited_scan = ScanResources::new(ScanLimits {
        references_per_document: 1,
        ..ScanLimits::CONTRACT
    });
    let mut limited_cache = TargetCache::default();
    let mut limited = Resolver::new(
        &repo,
        &mut git_resources,
        &mut limited_scan,
        &mut limited_cache,
        &discovery,
    );
    let before_ceiling = transcluded(&mut limited, "docs/host.adoc#_repeat_2");
    assert!(
        matches!(before_ceiling, Resolution::Resolved { .. }),
        "known identities before the edge ceiling remain evidence: {before_ceiling:?}"
    );
    let beyond_ceiling = transcluded(&mut limited, "docs/host.adoc#_deep");
    assert!(
        matches!(beyond_ceiling, Resolution::UnsupportedSemantics(_)),
        "an unexpanded edge leaves absence undecided: {beyond_ceiling:?}"
    );
}

/// The neighbor steps forward only alone: one typography match names itself,
/// two stay bare as a real ambiguity, and an unrelated miss stays bare.
#[test]
fn a_typography_neighbor_steps_forward_alone() {
    let mut bed = bed();
    let near_of = |bed: &mut crate::support::Bed, destination: &str| {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Missing(Missing::HeadingAnchorNotFound { near, .. }) = row else {
            panic!("{destination} is not a missing anchor: {row:?}");
        };
        near
    };
    assert_eq!(
        near_of(&mut bed, "anchors.md#Setup--Config"),
        Some("setup--config".to_owned()),
        "one case-fold neighbor names itself"
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#DECLARED"),
        Some("declared".to_owned())
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#customid"),
        None,
        "an unrelated miss stays bare"
    );
    assert_eq!(
        near_of(&mut bed, "case.md#DUP"),
        None,
        "two case variants are a real ambiguity"
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#anchor_point"),
        Some("anchor-point".to_owned()),
        "the separator renderers disagree on folds away"
    );
    assert_eq!(
        near_of(&mut bed, "anchors.md#Pair_ID"),
        Some("pair-id".to_owned()),
        "case and separator fold together"
    );
}

/// A path the tree does not hold names its one case neighbor, and stays bare
/// when nothing or more than one thing comes close.
#[test]
fn a_case_drifted_path_names_its_neighbor() {
    let mut bed = bed();
    let near_of = |bed: &mut crate::support::Bed, destination: &str| {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Missing(Missing::PathNotFound { near, .. }) = row else {
            panic!("{destination} is not a missing path: {row:?}");
        };
        near.and_then(|path| path.as_str().map(str::to_owned))
    };
    assert_eq!(
        near_of(&mut bed, "Anchors.md"),
        Some("docs/anchors.md".to_owned()),
        "a case-drifted basename names the tracked spelling"
    );
    assert_eq!(
        near_of(&mut bed, "nothing-like-this.md"),
        None,
        "a path nothing comes close to stays bare"
    );
}
