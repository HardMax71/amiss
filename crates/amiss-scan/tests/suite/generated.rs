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
    for line in [3, 9, 15] {
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
            answer(&rows, index, 7),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute)
        ),
        "{:?}",
        answer(&rows, index, 7)
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
