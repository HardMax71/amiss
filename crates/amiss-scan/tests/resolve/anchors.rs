use amiss_fixtures::{CommitChain, Staged, commit_chain, staged_repository};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::resolve::{Resolver, TargetCache};
use amiss_scan::{ScanLimits, ScanResources, discover};
use amiss_wire::model::{Adapter, ObjectFormat, Oid, RepoPath};
use amiss_wire::resolution::Resolution;
use amiss_wire::resolution::{Missing, Target, UnsupportedSemantics};

use crate::support::{ANCHORS, bed, bed_at, bed_with};

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
        "inline-id",
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

    for fragment in ["Setup--Config", "setup", "résumé", "customid", "cls"] {
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

/// MDN's rule is read only under the front-matter schema its repositories keep
/// at their root, so its underscore spelling answers for no other tree.
#[test]
fn the_mdn_rule_is_read_only_under_mdn_content() {
    for (marker, read) in [
        (None, false),
        (Some("front-matter-config.json"), true),
        (Some(".front-matter-config.json"), true),
    ] {
        let mut files = vec![
            ("files/guide.md", "# Setup Config\n"),
            ("files/index.md", "# Index\n"),
        ];
        files.extend(marker.map(|name| (name, "{}\n")));
        let chain = commit_chain(&[("tree", files.as_slice())])
            .unwrap_or_else(|_defect| panic!("commit {marker:?}"));
        let mut bed = bed_at(chain, 0, ScanLimits::CONTRACT, GitLimits::CONTRACT);
        let row = bed
            .run_as(
                Adapter::Markdown,
                None,
                "files/index.md",
                false,
                "guide.md#setup_config",
            )
            .unwrap_or_else(|_defect| panic!("resolve under {marker:?}"))
            .1;
        assert_eq!(
            matches!(row, Resolution::Resolved { .. }),
            read,
            "{marker:?}: {row:?}"
        );
    }
}

/// A text directive is text a browser finds on the page, never an identity,
/// so it is declined wherever it sits in the fragment.
#[test]
fn a_text_directive_is_declined_rather_than_missing() {
    let mut bed = bed();
    for destination in ["anchors.md#:~:text=setup", "anchors.md#setup:~:text=config"] {
        let row = bed
            .run_as(Adapter::Markdown, None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(
                row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
            ),
            "{destination}: {row:?}"
        );
    }
}

/// A chapter an `AsciiDoc` book includes, and an Antora partial, render inside
/// another page, so a fragment they do not hold themselves may name an
/// identity of that page and is declined. A page of its own still answers.
#[test]
fn an_included_asciidoc_chapter_declines_what_it_does_not_hold() {
    let files: &[(&str, &str)] = &[
        (
            "book.adoc",
            "= Book\n\ninclude::ch1.adoc[]\n\ninclude::ch2.adoc[]\n",
        ),
        ("ch1.adoc", "== Chapter One\n"),
        ("ch2.adoc", "[[ch2-sec]]\n== Chapter Two\n"),
        ("alone.adoc", "= Alone\n"),
        ("docs/antora.yml", "name: comp\nversion: ~\n"),
        ("docs/modules/ROOT/pages/index.adoc", "= Page\n"),
        ("docs/modules/ROOT/partials/bit.adoc", "Text.\n"),
    ];
    let chain = commit_chain(&[("book", files)]).unwrap_or_else(|_defect| panic!("commit"));
    let mut bed = bed_at(chain, 0, ScanLimits::CONTRACT, GitLimits::CONTRACT);
    for (document, declined) in [
        ("ch1.adoc", true),
        ("docs/modules/ROOT/partials/bit.adoc", true),
        ("alone.adoc", false),
        ("docs/modules/ROOT/pages/index.adoc", false),
    ] {
        let row = bed
            .run_as(Adapter::AsciiDoc, None, document, false, "#ch2-sec")
            .unwrap_or_else(|_defect| panic!("resolve in {document}"))
            .1;
        let unsupported = matches!(
            row,
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
        );
        let missing = matches!(
            row,
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        );
        assert!(
            if declined { unsupported } else { missing },
            "{document}: {row:?}"
        );
    }
}

/// Zola transliterates a heading to ASCII before it names it, which no rule
/// in the table does, so a page under Zola with a heading outside ASCII
/// declines what it does not publish. An ASCII heading still resolves, and a
/// page outside Zola still proves absence.
#[test]
fn a_zola_page_with_a_transliterated_heading_declines_what_it_lacks() {
    let files: &[(&str, &str)] = &[
        ("site/config.toml", "base_url = \"https://example.com\"\n"),
        ("site/content/ru.md", "# Привет мир\n\n## Setup\n"),
        ("site/content/_index.md", "# Index\n"),
        ("plain/ru.md", "# Привет мир\n"),
        ("plain/index.md", "# P\n"),
    ];
    let chain = commit_chain(&[("zola", files)]).unwrap_or_else(|_defect| panic!("commit"));
    let mut bed = bed_at(chain, 0, ScanLimits::CONTRACT, GitLimits::CONTRACT);
    for (document, destination, want) in [
        ("site/content/_index.md", "ru.md#privet-mir", "declined"),
        ("site/content/_index.md", "ru.md#setup", "resolved"),
        ("plain/index.md", "ru.md#privet-mir", "missing"),
    ] {
        let row = bed
            .run_as(Adapter::Markdown, None, document, false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let got = if matches!(row, Resolution::Resolved { .. }) {
            "resolved"
        } else if matches!(
            row,
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
        ) {
            "declined"
        } else if matches!(
            row,
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        ) {
            "missing"
        } else {
            "other"
        };
        assert_eq!(got, want, "{document}: {destination}");
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

/// What one target answers for one fragment, asked the way a reference in
/// `document` asks it: published, proven absent, or undecided because the
/// identity set the target could compute is incomplete.
#[expect(clippy::panic, reason = "test fixture helper")]
fn publishes(
    bed: &mut crate::support::Bed,
    adapter: Adapter,
    document: &str,
    target: &str,
    fragment: &str,
) -> Option<bool> {
    let destination = format!("{target}#{fragment}");
    let row = bed
        .run_as(adapter, None, document, false, &destination)
        .unwrap_or_else(|_defect| panic!("resolve {destination}"))
        .1;
    if let Resolution::Resolved {
        target: Target::Blob(blob),
    } = &row
    {
        assert_eq!(blob.path.as_str(), Some(target), "{fragment}");
        return Some(true);
    }
    if let Resolution::Missing(Missing::HeadingAnchorNotFound { path, .. }) = &row {
        assert_eq!(path.as_str(), Some(target), "{fragment}");
        return Some(false);
    }
    let Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_)) = &row else {
        panic!("{fragment} is not a heading-anchor answer: {row:?}");
    };
    None
}

/// One matrix over the MDX identity fixture, where each row is published,
/// proven absent, or undecided. `page.mdx` is the heading expression, where
/// Docusaurus escapes the classic `{#id}` before MDX parses the file and reads
/// the identity back out of the heading text, so the identity replaces the slug
/// and an expression declaring none leaves the slug standing. `element.mdx` is
/// the JSX `id`, read from a plain element, from one nested inside another, and
/// from the block a component wraps, which is this document's own Markdown,
/// but never from the component's own tag, because what a component renders
/// with a prop is unknown here. An `mdx-code-block` fence there is markup rather
/// than code, since Docusaurus strips the fence lines before anything parses
/// the file, while a fence naming any other language stays code and declares
/// nothing. `parent.mdx` is the partial, whose headings and whose own
/// partial's headings are the page's; the component it imports from a package
/// is no document, so absence there is still provable. Two documents rendering
/// each other leave absence undecided past the cycle.
#[test]
fn an_mdx_document_publishes_the_identities_it_writes_down() {
    let mut bed = bed_at(
        amiss_fixtures::mdx_identities().expect("the fixture stages"),
        0,
        ScanLimits::CONTRACT,
        GitLimits::CONTRACT,
    );
    for (target, fragment, answer) in [
        ("docs/page.mdx", "custom-id", Some(true)),
        ("docs/page.mdx", "value", Some(true)),
        ("docs/page.mdx", "price", Some(false)),
        ("docs/page.mdx", "late", Some(false)),
        ("docs/page.mdx", "missing-id", Some(false)),
        ("docs/element.mdx", "node-env", Some(true)),
        ("docs/element.mdx", "named", Some(true)),
        ("docs/element.mdx", "outer", Some(true)),
        ("docs/element.mdx", "inner", Some(true)),
        ("docs/element.mdx", "component", Some(false)),
        ("docs/element.mdx", "under-component", Some(true)),
        ("docs/element.mdx", "spliced", Some(true)),
        ("docs/element.mdx", "quoted", Some(false)),
        ("docs/element.mdx", "absent", Some(false)),
        ("docs/parent.mdx", "parent-id", Some(true)),
        ("docs/parent.mdx", "tags-file", Some(true)),
        ("docs/parent.mdx", "deep-id", Some(true)),
        ("docs/parent.mdx", "absent", Some(false)),
        ("docs/_tags.mdx", "tags-file", Some(true)),
        ("docs/_tags.mdx", "parent-id", Some(false)),
        ("docs/cycle-a.mdx", "a-id", Some(true)),
        ("docs/cycle-a.mdx", "b-id", Some(true)),
        ("docs/cycle-a.mdx", "absent", None),
    ] {
        assert_eq!(
            publishes(&mut bed, Adapter::Mdx, "guide.mdx", target, fragment),
            answer,
            "{target}#{fragment}"
        );
    }
}

/// The snippet syntax belongs to a mkdocs extension, so the line is read only
/// under a tree that declares mkdocs, and the path is resolved from the
/// directory holding that declaration rather than from beside the document. A
/// page whose whole body is one snippet publishes what the file it pulls in
/// publishes. A section coordinate names part of a file this engine cannot
/// reproduce and a target the tree lacks is unreadable, so both leave absence
/// undecided; the same line where no `mkdocs.yml` governs it includes nothing,
/// which leaves that page able to prove absence.
#[test]
fn a_mkdocs_snippet_publishes_the_identities_of_the_file_it_pulls_in() {
    let mut bed = bed_at(
        amiss_fixtures::mkdocs_snippets().expect("the fixture stages"),
        0,
        ScanLimits::CONTRACT,
        GitLimits::CONTRACT,
    );
    for (target, fragment, answer) in [
        ("site/docs/about/contributing.md", "installing", Some(true)),
        ("site/docs/about/contributing.md", "absent", Some(false)),
        ("site/docs/section.md", "installing", None),
        ("site/docs/gone.md", "installing", None),
        ("outside/notes.md", "installing", Some(false)),
    ] {
        assert_eq!(
            publishes(&mut bed, Adapter::Markdown, "README.md", target, fragment),
            answer,
            "{target}#{fragment}"
        );
    }
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
fn transcluded(resolver: &mut Resolver<'_>, destination: &str) -> Resolution<RepoPath> {
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
