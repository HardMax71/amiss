#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over the generated-identity fixtures"
)]

use std::collections::BTreeMap;

use amiss_fixtures::CommitChain;
use amiss_git::Repository;
use amiss_scan::anchor::DECLARATIONS;
use amiss_scan::pipeline::commit_pair;
use amiss_scan::route::{ROUTERS, Spelling};
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::model::occurrences;
use amiss_wire::resolution::{
    Missing, MissingTag, Resolution, ResolutionTag, Target, UnsupportedSemantics,
    UnsupportedSemanticsTag,
};

use crate::surface::{bare_shell, engine};

type Answers = BTreeMap<(String, u64), Resolution<RepoPath>>;

/// Every candidate occurrence of a one-commit fixture, keyed by the document
/// that writes it and the line it sits on.
fn answers(chain: &CommitChain) -> Answers {
    let commit = chain.commits.first().expect("the fixture holds one commit");
    let oid = Oid::new(ObjectFormat::Sha1, commit.id.clone()).expect("a SHA-1 commit name");
    let repo = Repository::open(chain.root(), ObjectFormat::Sha1).expect("the fixture opens");
    let built =
        commit_pair(&repo, &engine(), None, &bare_shell(), &oid, &oid).expect("the scan runs");
    assert!(
        built.envelope.payload.result.complete,
        "{:?}",
        built.envelope.payload.errors
    );
    built
        .envelope
        .payload
        .observations
        .iter()
        .filter_map(|row| occurrences(row).candidate)
        .map(|occurrence| {
            let document = occurrence
                .observation_id_input
                .document
                .as_str()
                .expect("fixture documents are text")
                .to_owned();
            (
                (document, occurrence.source_span.start_line),
                occurrence.resolution.clone(),
            )
        })
        .collect()
}

fn answer<'a>(answers: &'a Answers, document: &str, line: u64) -> &'a Resolution<RepoPath> {
    answers
        .get(&(document.to_owned(), line))
        .unwrap_or_else(|| panic!("{document}:{line} is an extracted reference"))
}

type Verdict = (
    ResolutionTag,
    Option<MissingTag>,
    Option<UnsupportedSemanticsTag>,
);

/// One answer as the tags the wire spells it with, which is deep enough to
/// tell a declared boundary from an absent anchor and shallow enough to write
/// a whole fixture's answers down.
fn verdict(resolution: &Resolution<RepoPath>) -> Verdict {
    let absence = if let Resolution::Missing(reason) = resolution {
        Some(MissingTag::from(reason))
    } else {
        None
    };
    let semantics = if let Resolution::UnsupportedSemantics(reason) = resolution {
        Some(UnsupportedSemanticsTag::from(reason))
    } else {
        None
    };
    (ResolutionTag::from(resolution), absence, semantics)
}

fn blob(resolution: &Resolution<RepoPath>) -> Option<&str> {
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = resolution
    else {
        return None;
    };
    blob.path.as_str()
}

/// A generator instruction, a heading a hook expands and a content tab an
/// extension slugs all publish identities built outside the tree, so the set
/// this engine reads is incomplete: the heading a page writes itself still
/// answers, and an anchor only the plugin knows is declared rather than
/// reported absent. An ordinary page under the same site still proves absence,
/// an admonition fence names no generator, and the same spellings where no
/// `mkdocs.yml` governs them are the text they look like.
#[test]
fn a_declared_plugin_leaves_the_identity_set_incomplete() {
    let chain = amiss_fixtures::mkdocs_generated().expect("the fixture stages");
    let rows = answers(&chain);
    assert_eq!(
        blob(answer(&rows, "README.md", 1)),
        Some("site/docs/api.md"),
        "the page writes its own title"
    );
    for (line, place) in [
        (3, "a generator instruction"),
        (11, "a heading a hook expands"),
        (13, "a content tab"),
    ] {
        assert!(
            matches!(
                answer(&rows, "README.md", line),
                Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
            ),
            "{place}: {:?}",
            answer(&rows, "README.md", line)
        );
    }
    for (line, place) in [
        (5, "an ordinary page under the same site"),
        (7, "an admonition fence"),
        (9, "an instruction no mkdocs governs"),
        (15, "a content tab no mkdocs governs"),
    ] {
        assert!(
            matches!(
                answer(&rows, "README.md", line),
                Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
            ),
            "{place}: {:?}",
            answer(&rows, "README.md", line)
        );
    }
}

/// A Hugo shortcode is answered by a layout rather than by a file, so a page
/// that calls one as a block holds content this engine cannot read: an anchor
/// naming what the call writes is declared rather than reported absent,
/// wherever the link sits. The page keeps every heading it writes itself, a
/// page under the same site that calls nothing still proves absence, a call in
/// the flow of a sentence leaves the set enumerable, and the same page outside
/// the site is read as the text it looks like. A call inside a heading writes
/// that heading's text, unless the heading sets its own id, and Eleventy's
/// Liquid in a heading is read the same way under its own configuration only.
#[test]
fn a_hugo_shortcode_leaves_the_identity_set_incomplete() {
    let rows = answers(&amiss_fixtures::hugo_shortcodes().expect("the fixture stages"));
    let store = "site/content/methods/Store.md";
    let boundary: Verdict = (
        ResolutionTag::UnsupportedSemantics,
        None,
        Some(UnsupportedSemanticsTag::Fragment),
    );
    let absent: Verdict = (
        ResolutionTag::Missing,
        Some(MissingTag::HeadingAnchorNotFound),
        None,
    );
    let published: Verdict = (ResolutionTag::Resolved, None, None);
    let want = BTreeMap::from([
        (("README.md".to_owned(), 1), boundary),
        (("README.md".to_owned(), 3), published),
        (("README.md".to_owned(), 5), absent),
        (("README.md".to_owned(), 7), absent),
        (("README.md".to_owned(), 9), absent),
        (("README.md".to_owned(), 11), boundary),
        (("README.md".to_owned(), 13), absent),
        (("README.md".to_owned(), 15), published),
        (("README.md".to_owned(), 17), boundary),
        (("README.md".to_owned(), 19), absent),
        ((store.to_owned(), 3), boundary),
        (("outside/Store.md".to_owned(), 3), absent),
    ]);
    let read: BTreeMap<(String, u64), Verdict> = rows
        .iter()
        .map(|(place, resolution)| (place.clone(), verdict(resolution)))
        .collect();
    assert_eq!(read, want);
    assert_eq!(blob(answer(&rows, "README.md", 3)), Some(store));
}

/// A definition-list term publishes an identity under the renderer that reads
/// one, so an anchor naming a term resolves, an anchor naming no term of a
/// page that writes a list is still absent, and the same anchor into a page
/// that writes no list at all stays absent too.
#[test]
fn a_definition_term_answers_an_anchor_that_names_it() {
    let chain = amiss_fixtures::definition_terms().expect("the fixture stages");
    let rows = answers(&chain);
    assert_eq!(
        blob(answer(&rows, "README.md", 1)),
        Some("docs/options.md"),
        "the page writes the term"
    );
    for (line, place) in [
        (3, "a term the page never wrote"),
        (5, "a page with no list"),
    ] {
        assert!(
            matches!(
                answer(&rows, "README.md", line),
                Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
            ),
            "{place}: {:?}",
            answer(&rows, "README.md", line)
        );
    }
}

/// Under a `conf.py`, a `MyST` role is the reference it spells: a docname takes
/// the profile's suffix and resolves like any path, a label resolves against
/// the names the tree declares, a docname or label nothing answers is
/// missing, a source-root docname stays a declared site route, and every
/// other role names a domain inventory outside the tree. The target a page
/// declares is an identity as well as a label, and the same role where no
/// `conf.py` governs it is prose.
#[test]
fn a_myst_role_is_read_only_where_sphinx_is_declared() {
    let chain = amiss_fixtures::sphinx_myst().expect("the fixture stages");
    let rows = answers(&chain);
    let index = "docs/index.md";
    for line in [3, 7, 9, 15] {
        assert_eq!(
            blob(answer(&rows, index, line)),
            Some("docs/quickstart.md"),
            "line {line}"
        );
    }
    assert!(
        matches!(
            answer(&rows, index, 5),
            Resolution::Missing(Missing::PathNotFound { .. })
        ),
        "{:?}",
        answer(&rows, index, 5)
    );
    assert!(
        matches!(
            answer(&rows, index, 11),
            Resolution::Missing(Missing::LabelNotDeclared)
        ),
        "{:?}",
        answer(&rows, index, 11)
    );
    assert!(
        matches!(
            answer(&rows, index, 13),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::ExternalInventory)
        ),
        "{:?}",
        answer(&rows, index, 13)
    );
    assert!(
        !rows
            .keys()
            .any(|(document, line)| document == "outside/notes.md" && *line == 3),
        "a brace before a code span is prose where no conf.py governs it"
    );
}

/// A plain link under the same `conf.py` names a label the way the role does,
/// in the destination or as a bare fragment. The tree answers first, so a
/// destination it holds stays that file even where another page declares the
/// same spelling as a label. A name nobody declares is still a missing target,
/// and outside the Sphinx tree the reading is off, so the same link there is
/// the path it looks like.
#[test]
fn a_plain_link_names_a_label_where_the_role_does() {
    let rows = answers(&amiss_fixtures::sphinx_myst().expect("the fixture stages"));
    for (line, spelling) in [
        (17, "a label in the destination"),
        (19, "a label as a bare fragment"),
        (23, "a destination the tree holds"),
    ] {
        assert_eq!(
            blob(answer(&rows, "docs/index.md", line)),
            Some("docs/quickstart.md"),
            "{spelling}: {:?}",
            answer(&rows, "docs/index.md", line)
        );
    }
    for (document, line, absence) in [
        ("docs/index.md", 21, "a name nobody declares"),
        ("outside/notes.md", 5, "a label link no conf.py governs"),
    ] {
        assert!(
            matches!(
                answer(&rows, document, line),
                Resolution::Missing(Missing::PathNotFound { .. })
            ),
            "{absence}: {:?}",
            answer(&rows, document, line)
        );
    }
}

/// The names a `MyST` document writes down beside its targets: a directive's
/// `:name:` option, the argument `figure-md` takes instead, the same option
/// inside an `eval-rst` body, a glossary term under its own attribute block, a
/// bracketed span, and an attribute block that shares the opener's paragraph.
/// Each joins the label table, so a link naming one resolves to the page that
/// writes it. A fence with no brace tag opens no directive and a definition
/// list that is no glossary publishes no term, so both stay missing, and a name
/// declared where no `conf.py` governs it is never a label at all.
#[test]
fn a_name_a_directive_publishes_answers_a_link_that_spells_it() {
    let rows = answers(&amiss_fixtures::sphinx_myst().expect("the fixture stages"));
    for (line, spelling) in [
        (25, "a directive's own option"),
        (27, "a `figure-md` argument as a bare fragment"),
        (29, "an option inside an embedded reStructuredText body"),
        (31, "a glossary term"),
        (33, "a bracketed span"),
        (35, "a block sharing the opener's paragraph"),
    ] {
        assert_eq!(
            blob(answer(&rows, "docs/index.md", line)),
            Some("docs/widgets.md"),
            "{spelling}: {:?}",
            answer(&rows, "docs/index.md", line)
        );
    }
    for (line, absence) in [
        (37, "a fence with no brace tag"),
        (41, "a name no conf.py governs"),
    ] {
        assert!(
            matches!(
                answer(&rows, "docs/index.md", line),
                Resolution::Missing(Missing::PathNotFound { .. })
            ),
            "{absence}: {:?}",
            answer(&rows, "docs/index.md", line)
        );
    }
    assert!(
        matches!(
            answer(&rows, "docs/index.md", 39),
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        ),
        "a definition list that is no glossary: {:?}",
        answer(&rows, "docs/index.md", 39)
    );
}

/// A Sphinx domain directive stores the object it names under that name, so a
/// link spelling it reaches the page describing it. A parameter list is the
/// domain's own grammar and comes off the end, while a signature carrying a
/// space is left alone, and a directive with no domain in its tag names no
/// object at all.
#[test]
fn a_domain_directive_publishes_the_object_it_names() {
    let rows = answers(&amiss_fixtures::sphinx_myst().expect("the fixture stages"));
    for (line, spelling) in [
        (43, "a class written whole"),
        (45, "a name before its list"),
    ] {
        assert_eq!(
            blob(answer(&rows, "docs/index.md", line)),
            Some("docs/widgets.md"),
            "{spelling}: {:?}",
            answer(&rows, "docs/index.md", line)
        );
    }
    assert!(
        matches!(
            answer(&rows, "docs/index.md", 47),
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        ),
        "a signature this engine does not parse: {:?}",
        answer(&rows, "docs/index.md", 47)
    );
}

/// A page under the `conf.py` renders another file in place of an include, so
/// Sphinx parses that file too and the labels it writes are read wherever it
/// sits. A file outside the root that no page includes keeps the reading it
/// had, so the same link there is the path it looks like.
#[test]
fn a_file_a_sphinx_page_includes_reads_the_labels_it_writes() {
    let rows = answers(&amiss_fixtures::sphinx_myst().expect("the fixture stages"));
    for (line, spelling) in [
        (3, "a label in the destination"),
        (5, "a label as a bare fragment"),
    ] {
        assert_eq!(
            blob(answer(&rows, "CHANGELOG.md", line)),
            Some("docs/quickstart.md"),
            "{spelling}: {:?}",
            answer(&rows, "CHANGELOG.md", line)
        );
    }
}

/// The `MyST` rows and the route rule that anchors a source-root docname are
/// the same declaration, so the file each names must stay one file.
#[test]
fn the_myst_rows_name_the_file_the_route_table_reads() {
    let routed: Vec<&str> = ROUTERS
        .iter()
        .filter(|rule| rule.serves(Spelling::SourceRoot))
        .flat_map(|rule| rule.declared_by.iter().copied())
        .collect();
    let declared: Vec<&str> = DECLARATIONS
        .iter()
        .filter(|rule| rule.name == "myst-role")
        .flat_map(|rule| rule.declared_by.iter().copied())
        .collect();
    assert_eq!(routed, declared, "the Sphinx declaration is one file");
    assert!(!routed.is_empty(), "the route table declares Sphinx");
}

/// Sphinx declares `genindex`, `modindex`, `py-modindex` and `search` for the
/// pages every build writes, so a `:ref:` to one is the build's own inventory
/// rather than a missing label, while a name nobody declares stays missing.
#[test]
fn a_sphinx_built_in_label_is_not_missing() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "project = 'probe'\n"),
            (
                "docs/index.rst",
                ":ref:`genindex`\n\n:ref:`py-modindex`\n\n:ref:`Search <search>`\n\n:ref:`nowhere`\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for line in [1, 3, 5] {
        assert_eq!(
            answer(&rows, "docs/index.rst", line),
            &Resolution::UnsupportedSemantics(UnsupportedSemantics::ExternalInventory),
            "line {line}"
        );
    }
    assert_eq!(
        answer(&rows, "docs/index.rst", 7),
        &Resolution::Missing(Missing::LabelNotDeclared)
    );
}

/// `autosectionlabel` declares every section title as a label, prefixed with
/// the docname where the configuration asks, and `autodoc` declares labels in
/// Python docstrings no document holds, so a name nothing here declares is
/// declined under it. The same name under a configuration loading neither
/// stays missing.
#[test]
fn a_sphinx_extension_declares_the_labels_it_builds() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            (
                "a/conf.py",
                "extensions = [\n    \"sphinx.ext.autodoc\",\n    'sphinx.ext.autosectionlabel',\n]\nautosectionlabel_prefix_document = True\n",
            ),
            ("a/guide.rst", "Guide\n=====\n\nInstall Steps\n-------------\n"),
            (
                "a/index.rst",
                ":ref:`guide:Install Steps`\n\n:ref:`docstring-label`\n",
            ),
            ("b/conf.py", "extensions = ['sphinx.ext.intersphinx']\n"),
            ("b/index.rst", ":ref:`docstring-label`\n"),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    assert!(matches!(
        answer(&rows, "a/index.rst", 1),
        Resolution::Resolved { .. }
    ));
    assert_eq!(
        answer(&rows, "a/index.rst", 3),
        &Resolution::UnsupportedSemantics(UnsupportedSemantics::ExternalInventory)
    );
    assert_eq!(
        answer(&rows, "b/index.rst", 1),
        &Resolution::Missing(Missing::LabelNotDeclared)
    );
}

/// A `:download:` names a file beside its document, or under the source root
/// when it opens with a slash, and a `:numref:` is a label like a `:ref:`.
#[test]
fn a_download_names_a_file_and_a_numref_a_label() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "project = 'probe'\n"),
            ("docs/files/data.csv", "a,b\n"),
            ("docs/guide/run.py", "print()\n"),
            (
                "docs/guide/index.rst",
                ".. _fig-arch:\n\nGuide\n=====\n\n:download:`run.py`\n\n:download:`/files/data.csv`\n\n:download:`gone.py`\n\n:numref:`fig-arch`\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for line in [6, 8, 12] {
        assert!(
            matches!(
                answer(&rows, "docs/guide/index.rst", line),
                Resolution::Resolved { .. }
            ),
            "line {line}"
        );
    }
    assert!(matches!(
        answer(&rows, "docs/guide/index.rst", 10),
        Resolution::Missing(Missing::PathNotFound { .. })
    ));
}

/// Sphinx reads every relative path an included file writes from the page
/// that includes it, a nested include's too, so a fragment beside nothing it
/// names still reaches the page's files and one the page lacks is missing
/// there.
#[test]
fn an_included_fragment_reads_from_its_page() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "project = 'probe'\n"),
            ("docs/index.rst", "Index\n=====\n\n.. include:: sub/frag.rst\n"),
            ("docs/other.rst", "Other\n=====\n"),
            ("docs/data.txt", "data\n"),
            ("docs/pic.png", "png\n"),
            ("docs/part.rst", "Part.\n"),
            (
                "docs/sub/frag.rst",
                ":doc:`other`\n\n:download:`data.txt`\n\n.. image:: pic.png\n\n.. include:: part.rst\n\n.. image:: gone.png\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for (line, target) in [
        (1, "docs/other.rst"),
        (3, "docs/data.txt"),
        (5, "docs/pic.png"),
        (7, "docs/part.rst"),
    ] {
        assert_eq!(
            blob(answer(&rows, "docs/sub/frag.rst", line)),
            Some(target),
            "line {line}"
        );
    }
    assert!(matches!(
        answer(&rows, "docs/sub/frag.rst", 9),
        Resolution::Missing(Missing::PathNotFound { path, .. }) if path.as_bytes() == b"docs/gone.png"
    ));
}

/// Sphinx writes each page as `.html` beside where its source sits, so a
/// relative link to that page reaches the source under any suffix the root
/// reads, fragment and all, and one naming no source is missing.
#[test]
fn a_sphinx_html_link_reaches_its_source() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "extensions = ['myst_parser']\n"),
            ("docs/a.rst", "A\n=\n\nX section\n---------\n"),
            ("docs/m.md", "# M\n"),
            (
                "docs/b.rst",
                "B\n=\n\n`A <a.html>`_\n\n`Sec <a.html#x-section>`_\n\n`M <m.html>`_\n\n`Gone <gone.html>`_\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for (line, target) in [(4, "docs/a.rst"), (6, "docs/a.rst"), (8, "docs/m.md")] {
        assert_eq!(
            blob(answer(&rows, "docs/b.rst", line)),
            Some(target),
            "line {line}"
        );
    }
    assert!(matches!(
        answer(&rows, "docs/b.rst", 10),
        Resolution::Missing(Missing::PathNotFound { .. })
    ));
}

/// Under a Sphinx declaration a `MyST` directive that names a file is a
/// reference the way the reStructuredText directive is, and so is the
/// `{download}` role; outside one the fence is the code block it looks like.
#[test]
fn a_myst_file_directive_is_a_reference_under_sphinx() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "extensions = ['myst_parser']\n"),
            ("docs/img/pic.png", "png\n"),
            ("docs/img/fig.png", "png\n"),
            ("docs/part.md", "Part.\n"),
            ("docs/data.csv", "a\n"),
            ("src/code.py", "print()\n"),
            (
                "docs/index.md",
                "# Index\n\n```{image} img/pic.png\n```\n\n:::{figure} /img/fig.png\nCaption\n:::\n\n```{literalinclude} ../src/code.py\n```\n\n```{include} part.md\n```\n\n{download}`data.csv`\n\n```{image} img/gone.png\n```\n",
            ),
            ("plain/page.md", "# Page\n\n```{image} gone.png\n```\n"),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for (line, target) in [
        (3, "docs/img/pic.png"),
        (6, "docs/img/fig.png"),
        (10, "src/code.py"),
        (13, "docs/part.md"),
        (16, "docs/data.csv"),
    ] {
        assert_eq!(
            blob(answer(&rows, "docs/index.md", line)),
            Some(target),
            "line {line}"
        );
    }
    assert!(matches!(
        answer(&rows, "docs/index.md", 18),
        Resolution::Missing(Missing::PathNotFound { .. })
    ));
    assert!(!rows.keys().any(|(document, _)| document == "plain/page.md"));
}

/// Antora reads a partial's cross references and images from the module of
/// each page that includes it and its includes from the partial itself, so a
/// partial another module includes misses what only its own module holds. An
/// include selecting a tag renders only part of the partial, so it leaves the
/// partial read from its own module.
#[test]
fn an_antora_partial_reads_from_its_pages_module() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/antora.yml", "name: comp\nversion: ~\n"),
            (
                "docs/modules/ROOT/pages/index.adoc",
                "= Index\n\ninclude::partial$shared.adoc[]\n",
            ),
            ("docs/modules/ROOT/pages/other.adoc", "= Other\n"),
            ("docs/modules/ROOT/images/pic.png", "png\n"),
            (
                "docs/modules/ROOT/partials/shared.adoc",
                "See xref:other.adoc[].\n\nimage::pic.png[]\n\ninclude::partial$nested.adoc[]\n\ninclude::sibling.adoc[]\n",
            ),
            ("docs/modules/ROOT/partials/nested.adoc", "Nested.\n"),
            ("docs/modules/ROOT/partials/sibling.adoc", "Sibling.\n"),
            (
                "docs/modules/ROOT/partials/tagged.adoc",
                "// tag::intro[]\nIntro.\n// end::intro[]\n\nimage::pic.png[]\n",
            ),
            (
                "docs/modules/extra/pages/page.adoc",
                "= Extra\n\ninclude::ROOT:partial$shared.adoc[]\n\ninclude::ROOT:partial$tagged.adoc[tag=intro]\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    assert_eq!(
        blob(answer(&rows, "docs/modules/ROOT/partials/tagged.adoc", 5)),
        Some("docs/modules/ROOT/images/pic.png")
    );
    let partial = "docs/modules/ROOT/partials/shared.adoc";
    for (line, missing) in [
        (1, "docs/modules/extra/pages/other.adoc"),
        (3, "docs/modules/extra/images/pic.png"),
    ] {
        assert!(
            matches!(
                answer(&rows, partial, line),
                Resolution::Missing(Missing::PathNotFound { path, .. }) if path.as_bytes() == missing.as_bytes()
            ),
            "line {line}: {:?}",
            answer(&rows, partial, line)
        );
    }
    for (line, target) in [
        (5, "docs/modules/ROOT/partials/nested.adoc"),
        (7, "docs/modules/ROOT/partials/sibling.adoc"),
    ] {
        assert_eq!(
            blob(answer(&rows, partial, line)),
            Some(target),
            "line {line}"
        );
    }
}

/// A file two pages include has to reach its target from both, so the page
/// whose directory lacks it is where it is missing.
#[test]
fn an_included_fragment_answers_to_every_page() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "project = 'probe'\n"),
            (
                "docs/a/index.rst",
                "A\n=\n\n.. include:: ../shared/frag.rst\n",
            ),
            (
                "docs/b/index.rst",
                "B\n=\n\n.. include:: ../shared/frag.rst\n",
            ),
            ("docs/a/pic.png", "png\n"),
            ("docs/shared/frag.rst", ".. image:: pic.png\n"),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    assert!(
        matches!(
            answer(&rows, "docs/shared/frag.rst", 1),
            Resolution::Missing(Missing::PathNotFound { path, .. }) if path.as_bytes() == b"docs/b/pic.png"
        ),
        "{:?}",
        answer(&rows, "docs/shared/frag.rst", 1)
    );
}

/// Sphinx finds a docname's file under any suffix its root reads, so a
/// `:doc:` in reStructuredText reaches a `MyST` page and a `{doc}` in `MyST`
/// reaches a reStructuredText one, relative or from the root.
#[test]
fn a_docname_crosses_formats_in_a_mixed_root() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            (
                "docs/conf.py",
                "extensions = ['myst_parser']\nsource_suffix = {'.rst': 'restructuredtext', '.md': 'markdown'}\n",
            ),
            ("docs/a.rst", "A\n=\n\n:doc:`b`\n\n:doc:`/sub/c`\n\n:doc:`gone`\n"),
            ("docs/b.md", "# B\n\n{doc}`a`\n\n{doc}`sub/c`\n"),
            ("docs/sub/c.md", "# C\n\n{doc}`../a`\n\n{doc}`/b`\n"),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for (document, line, target) in [
        ("docs/a.rst", 4, "docs/b.md"),
        ("docs/a.rst", 6, "docs/sub/c.md"),
        ("docs/b.md", 3, "docs/a.rst"),
        ("docs/b.md", 5, "docs/sub/c.md"),
        ("docs/sub/c.md", 3, "docs/a.rst"),
        ("docs/sub/c.md", 5, "docs/b.md"),
    ] {
        assert_eq!(
            blob(answer(&rows, document, line)),
            Some(target),
            "{document}:{line}"
        );
    }
    assert!(matches!(
        answer(&rows, "docs/a.rst", 8),
        Resolution::Missing(Missing::PathNotFound { .. })
    ));
}

/// A Jekyll `link` or `post_url` tag names a file under the site source, and a
/// Hugo `ref` names a page beside the page, under the language's content or
/// by its bare name in any case; one naming nothing fails the build, so it is missing,
/// while the same tag with no generator declared above it is undecided.
#[test]
fn a_generator_template_names_a_file_of_its_site() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("site/_config.yml", "title: probe\n"),
            ("site/_docs/data.md", "# Data\n"),
            ("site/_posts/2020-01-02-hello.markdown", "# Hello\n"),
            (
                "site/guide.md",
                "[a]({% link _docs/data.md %})\n\n[b]({% post_url 2020-01-02-hello %})\n\n[c]({% link _docs/gone.md %})\n",
            ),
            ("web/hugo.toml", "title = 'probe'\n"),
            ("web/content/en/docs/releases.md", "# Releases\n\n## Cadence\n"),
            ("web/content/en/community/_index.md", "# Community\n"),
            (
                "web/content/en/docs/guide.md",
                "[a]({{< ref \"releases.md#cadence\" >}})\n\n[b]({{<relref community >}})\n\n[c]({{< relref \"/docs/releases\" >}})\n\n[d]({{< ref \"gone.md\" >}})\n\n[e]({{< ref \"Community\" >}})\n",
            ),
            ("loose/page.md", "[a]({% link _docs/data.md %})\n"),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for (document, line) in [
        ("site/guide.md", 1),
        ("site/guide.md", 3),
        ("web/content/en/docs/guide.md", 1),
        ("web/content/en/docs/guide.md", 3),
        ("web/content/en/docs/guide.md", 5),
        ("web/content/en/docs/guide.md", 9),
    ] {
        assert!(
            matches!(answer(&rows, document, line), Resolution::Resolved { .. }),
            "{document}:{line}: {:?}",
            answer(&rows, document, line)
        );
    }
    for (document, line) in [("site/guide.md", 5), ("web/content/en/docs/guide.md", 7)] {
        assert!(
            matches!(
                answer(&rows, document, line),
                Resolution::Missing(Missing::PathNotFound { .. })
            ),
            "{document}:{line}: {:?}",
            answer(&rows, document, line)
        );
    }
    assert!(matches!(
        answer(&rows, "loose/page.md", 1),
        Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent)
    ));
}

/// A `:term:` names a glossary term any page of the root declares, in any
/// case. One nobody declares is missing, unless intersphinx is loaded, since
/// then it may be another project's term.
#[test]
fn a_term_role_names_a_declared_glossary_term() {
    for (config, undeclared_missing) in [
        ("project = 'probe'\n", true),
        ("extensions = ['sphinx.ext.intersphinx']\n", false),
    ] {
        let chain = amiss_fixtures::commit_chain(&[(
            "base",
            &[
                ("docs/conf.py", config),
                (
                    "docs/glossary.rst",
                    "Glossary\n========\n\n.. glossary::\n\n   source directory\n      Where conf.py lives.\n",
                ),
                (
                    "docs/index.rst",
                    "Index\n=====\n\n:term:`source directory`\n\n:term:`Source Directory`\n\n:term:`iterable`\n",
                ),
            ],
        )])
        .expect("the fixture stages");
        let rows = answers(&chain);
        for line in [4, 6] {
            assert!(
                matches!(
                    answer(&rows, "docs/index.rst", line),
                    Resolution::Resolved { .. }
                ),
                "line {line}"
            );
        }
        let undeclared = answer(&rows, "docs/index.rst", 8);
        assert_eq!(
            matches!(undeclared, Resolution::Missing(Missing::LabelNotDeclared)),
            undeclared_missing,
            "{config}: {undeclared:?}"
        );
    }
}

/// Sphinx reads an image, figure, include or literalinclude path that starts
/// with `/` from the directory holding `conf.py`, not as a site route.
#[test]
fn a_directive_slash_path_starts_at_the_source_root() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "project = 'probe'\n"),
            ("docs/_static/logo.png", "png\n"),
            ("docs/snippets/part.rst", "Part text.\n"),
            ("docs/code/example.py", "print()\n"),
            (
                "docs/guide/index.rst",
                "Guide\n=====\n\n.. image:: /_static/logo.png\n\n.. figure:: /_static/gone.png\n\n.. include:: /snippets/part.rst\n\n.. literalinclude:: /code/example.py\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for line in [4, 8, 10] {
        assert!(
            matches!(
                answer(&rows, "docs/guide/index.rst", line),
                Resolution::Resolved { .. }
            ),
            "line {line}"
        );
    }
    assert!(matches!(
        answer(&rows, "docs/guide/index.rst", 6),
        Resolution::Missing(Missing::PathNotFound { .. })
    ));
}

/// A `literalinclude` selection is checked the way a line fragment is: a
/// `:lines:` range the file does not hold is out of range, one it holds
/// resolves, and a `:pyobject:` is a code fragment the run declines.
#[test]
fn a_literalinclude_selection_is_checked_as_a_line_range() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docs/conf.py", "project = 'probe'\n"),
            ("docs/code.py", "a = 1\nb = 2\nc = 3\n"),
            (
                "docs/index.rst",
                "Index\n=====\n\n.. literalinclude:: code.py\n   :lines: 2-3\n\n.. literalinclude:: code.py\n   :lines: 5-8\n\n.. literalinclude:: code.py\n   :pyobject: Foo\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    assert!(matches!(
        answer(&rows, "docs/index.rst", 4),
        Resolution::Resolved { .. }
    ));
    assert!(matches!(
        answer(&rows, "docs/index.rst", 7),
        Resolution::Missing(Missing::LineFragmentOutOfRange { .. })
    ));
    assert!(matches!(
        answer(&rows, "docs/index.rst", 10),
        Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(_))
    ));
}

/// An `AsciiDoc` section named with an attribute the document defines takes
/// the identity its value gives it, and a page whose title names one the
/// document leaves undefined cannot prove any other identity absent, since
/// whatever builds the site may define it.
#[test]
fn an_asciidoc_title_reads_its_documents_attributes() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            (
                "defined.adoc",
                "= Defined\n:product: Acme Cloud\n\n== Using {product}\n\nSee <<_using_acme_cloud>>.\n\nSee <<_nothing_here>>.\n",
            ),
            (
                "open.adoc",
                "= Open\n\n== About {edition}\n\nSee <<_nothing_here>>.\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    assert!(matches!(
        answer(&rows, "defined.adoc", 6),
        Resolution::Resolved { .. }
    ));
    assert!(matches!(
        answer(&rows, "defined.adoc", 8),
        Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
    ));
    assert!(
        !matches!(answer(&rows, "open.adoc", 5), Resolution::Missing(_)),
        "{:?}",
        answer(&rows, "open.adoc", 5)
    );
}

/// The HTML standard answers `#top`, in any case, with the top of the page, so
/// it names no heading and is never missing; a longer name still is.
#[test]
fn the_top_fragment_always_resolves() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            (
                "doc.md",
                "# Doc\n\n[a](#top)\n\n[b](#TOP)\n\n[c](other.md#Top)\n\n[d](#topmost)\n",
            ),
            ("other.md", "# Other\n"),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    for line in [3, 5, 7] {
        assert!(
            matches!(answer(&rows, "doc.md", line), Resolution::Resolved { .. }),
            "line {line}"
        );
    }
    assert!(matches!(
        answer(&rows, "doc.md", 9),
        Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
    ));
}

/// Docusaurus reads a bare Markdown link on a translated page under the
/// locale's content path and then the default one, so a page the locale has
/// not translated is still reached, and it reads a Markdown link opening with
/// `/` from its content paths and an image opening with `/` from `static`
/// where it sits there, rather than either as a URL. A `./` link stays beside
/// the page.
#[test]
fn a_docusaurus_link_falls_back_to_the_default_content() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            ("docusaurus.config.js", "export default {title: 'x'};\n"),
            ("docs/guides/other.md", "# Other\n\n## Deep\n"),
            ("static/img/logo.png", "png\n"),
            (
                "docs/intro.md",
                "# Intro\n\n[x](/guides/other.md)\n\n[y](/guides/gone.md)\n\n![l](/img/logo.png)\n\n![g](/img/gone.png)\n",
            ),
            (
                "i18n/fr/docusaurus-plugin-content-docs/current/intro.md",
                "# Intro FR\n\n[a](guides/other.md)\n\n[b](guides/other.md#deep)\n\n[c](./guides/other.md)\n\n[d](/guides/other.md)\n",
            ),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    let translated = "i18n/fr/docusaurus-plugin-content-docs/current/intro.md";
    for (document, line) in [
        (translated, 3),
        (translated, 5),
        (translated, 9),
        ("docs/intro.md", 3),
    ] {
        assert_eq!(
            blob(answer(&rows, document, line)),
            Some("docs/guides/other.md"),
            "{document}:{line}"
        );
    }
    assert_eq!(
        blob(answer(&rows, "docs/intro.md", 7)),
        Some("static/img/logo.png")
    );
    assert!(matches!(
        answer(&rows, "docs/intro.md", 9),
        Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute)
    ));
    for (document, line) in [(translated, 7), ("docs/intro.md", 5)] {
        assert!(
            matches!(
                answer(&rows, document, line),
                Resolution::Missing(Missing::PathNotFound { .. })
            ),
            "{document}:{line}: {:?}",
            answer(&rows, document, line)
        );
    }
}

/// A reference whose label no definition declares is a missing label, while
/// under a `mkdocs.yml`, where mkdocs-autorefs answers labels from the site's
/// own inventory, it is declined rather than guessed absent.
#[test]
fn an_undefined_reference_label_is_missing() {
    let chain = amiss_fixtures::commit_chain(&[(
        "base",
        &[
            (
                "plain/guide.md",
                "# Guide\n\nSee [Raft][raft].\n\n[raft-paper]: raft.md\n",
            ),
            ("site/mkdocs.yml", "site_name: probe\n"),
            ("site/docs/api.md", "# API\n\nSee [Model][pkg.Model].\n"),
        ],
    )])
    .expect("the fixture stages");
    let rows = answers(&chain);
    assert!(matches!(
        answer(&rows, "plain/guide.md", 3),
        Resolution::Missing(Missing::LabelNotDeclared)
    ));
    assert!(matches!(
        answer(&rows, "site/docs/api.md", 3),
        Resolution::UnsupportedSemantics(UnsupportedSemantics::ExternalInventory)
    ));
}
