#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over the published route-spelling vectors"
)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use amiss_fixtures::CommitChain;
use amiss_git::Repository;
use amiss_scan::pipeline::commit_pair;
use amiss_scan::route::{DECLARABLE, ROUTERS, RouteRule, Spelling, candidates, spellings};
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::model::{ObservedOccurrence, occurrences};
use amiss_wire::resolution::{Missing, Resolution, ResolutionTag, Target};
use serde_json::Value;

use crate::surface::{bare_shell, engine};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectors() -> Value {
    let bytes = fs::read(root().join("spec/examples/route-spelling-vectors.json"))
        .expect("the specification ships the route-spelling vectors");
    serde_json::from_slice::<Value>(&bytes).expect("route-spelling vectors are JSON")
}

fn path(raw: &str) -> RepoPath {
    RepoPath::new(raw.to_owned()).unwrap_or_else(|| panic!("{raw} is a repository path"))
}

fn served(rule: &RouteRule, destination: &str, tree: &BTreeSet<String>) -> Option<String> {
    spellings(rule, &path(destination))
        .into_iter()
        .filter_map(|(_, candidate)| candidate.as_str().map(str::to_owned))
        .find(|candidate| tree.contains(candidate))
}

/// A destination the tree answers by itself, either as a file or as a
/// directory, never reaches the route rules.
fn answered_by_the_tree(destination: &str, tree: &BTreeSet<String>) -> bool {
    let prefix = destination.strip_suffix('/').unwrap_or(destination);
    tree.contains(destination)
        || tree
            .iter()
            .any(|entry| entry.starts_with(&format!("{prefix}/")))
}

/// Every verdict came from the router. For each destination the tree does not
/// answer on its own, the rule reproduces the source file that router served,
/// or serves nothing where the router served nothing.
#[test]
fn the_published_vectors_drive_every_router() {
    let vectors = vectors();
    assert_eq!(
        vectors["schema"].as_str().expect("vector schema"),
        "amiss/route-spelling-vectors"
    );
    assert_eq!(
        vectors["contract"].as_str().expect("vector contract"),
        "route-spelling"
    );

    let tree: BTreeSet<String> =
        serde_json::from_value(vectors["tree"].clone()).expect("tree paths");
    let cases = vectors["cases"].as_array().expect("route cases");
    assert!(cases.len() >= 14, "the corpus keeps its probe set");
    let mut seen = BTreeSet::new();
    let mut asked = 0_usize;
    for case in cases {
        let id = case["id"].as_str().expect("case id");
        assert!(seen.insert(id), "case {id} is unique");
        let destination = case["destination"].as_str().expect("case destination");
        if answered_by_the_tree(destination, &tree) {
            continue;
        }
        asked = asked.saturating_add(1);
        let harvest = case["serves"].as_object().expect("router outcomes");
        for (name, found) in harvest {
            let rule = ROUTERS
                .iter()
                .find(|rule| rule.name == name)
                .unwrap_or_else(|| panic!("{name} is a known router"));
            let want: Option<String> =
                serde_json::from_value(found.clone()).expect("served source is a string or null");
            assert_eq!(
                served(rule, destination, &tree),
                want,
                "case {id} under {name}: destination {destination:?}"
            );
        }
    }
    assert_eq!(asked, 9, "the tree answers five of the fourteen by itself");
}

/// The union is what the resolver asks, so every rule's spelling is inside it
/// and no rule can remove one.
#[test]
fn the_union_holds_every_rule() {
    for destination in [
        "docs/presets/plain-text",
        "guide/configuration/index.html",
        "book/first/nested.html",
        "docs/withreadme/index.md",
    ] {
        let union = candidates(&path(destination));
        for rule in &ROUTERS {
            for (_, candidate) in spellings(rule, &path(destination)) {
                assert!(
                    union.iter().any(|(_, held)| *held == candidate),
                    "{} offers {candidate:?} for {destination}",
                    rule.name
                );
            }
        }
    }
}

#[test]
fn a_spelling_never_repeats_the_destination_or_itself() {
    for destination in [
        "docs/page.md",
        "docs/page",
        "docs/page.html",
        "docs/index.html",
    ] {
        let union = candidates(&path(destination));
        let mut held = BTreeSet::new();
        for (_, candidate) in &union {
            assert_ne!(candidate.as_str(), Some(destination), "{destination}");
            assert!(held.insert(candidate.clone()), "{destination} repeats");
        }
    }
}

/// mkdocs demands the source spelling in a link it rewrites, so it offers the
/// union no candidate: its rule moves where a destination is read from, not
/// how the source file is named.
#[test]
fn a_router_that_serves_no_spelling_offers_no_candidate() {
    let mkdocs = ROUTERS
        .iter()
        .find(|rule| rule.name == "mkdocs")
        .unwrap_or_else(|| panic!("mkdocs is a known router"));
    for destination in ["docs/page", "docs/page.html", "docs/dir/index.md"] {
        assert!(
            spellings(mkdocs, &path(destination)).is_empty(),
            "{destination}"
        );
    }
}

/// A bare name is offered under every suffix a page source carries, and a
/// destination already carrying a source or output name is offered none, so
/// neither `page.md.md` nor `page.mdx.md` is ever looked up.
#[test]
fn the_elided_extension_only_applies_to_a_bare_name() {
    let vitepress = ROUTERS
        .iter()
        .find(|rule| rule.name == "vitepress")
        .unwrap_or_else(|| panic!("vitepress is a known router"));
    for (destination, want) in [
        (
            "docs/page",
            vec!["docs/page.md", "docs/page.mdx", "docs/page.markdown"],
        ),
        ("docs/page.md", Vec::new()),
        ("docs/page.mdx", Vec::new()),
        ("docs/page.markdown", Vec::new()),
        ("docs/page.html", Vec::new()),
    ] {
        let offered: Vec<String> = spellings(vitepress, &path(destination))
            .into_iter()
            .filter(|(spelling, _)| *spelling == Spelling::Extensionless)
            .filter_map(|(_, candidate)| candidate.as_str().map(str::to_owned))
            .collect();
        assert_eq!(offered, want, "{destination}");
    }
}

type Outcome = (String, Option<String>, ResolutionTag, Option<String>);

/// One row per candidate occurrence of a one-commit fixture: the document,
/// the path its intent names, the resolution kind, and the path that answered
/// or went missing, in a fixed order.
fn outcomes(chain: &CommitChain) -> Vec<Outcome> {
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
    let mut rows: Vec<Outcome> = built
        .envelope
        .payload
        .observations
        .iter()
        .filter_map(|row| occurrences(row).candidate)
        .map(outcome)
        .collect();
    sort(&mut rows);
    rows
}

fn outcome(occurrence: &ObservedOccurrence<RepoPath, Resolution<RepoPath>>) -> Outcome {
    let text = |path: &RepoPath| path.as_str().map(str::to_owned);
    let answered = if let Resolution::Resolved {
        target: Target::Blob(blob),
    } = &occurrence.resolution
    {
        text(&blob.path)
    } else if let Resolution::Missing(Missing::PathNotFound { path, .. }) = &occurrence.resolution {
        text(path)
    } else {
        None
    };
    let identity = &occurrence.observation_id_input;
    (
        text(&identity.document).expect("fixture documents are text"),
        identity
            .extracted_intent
            .repository_path
            .as_ref()
            .and_then(text),
        ResolutionTag::from(&occurrence.resolution),
        answered,
    )
}

fn row(
    document: &str,
    intent: Option<&str>,
    tag: ResolutionTag,
    answered: Option<&str>,
) -> Outcome {
    (
        document.to_owned(),
        intent.map(str::to_owned),
        tag,
        answered.map(str::to_owned),
    )
}

fn sort(rows: &mut [Outcome]) {
    rows.sort_by(|left, right| {
        (&left.0, &left.1, left.2.as_ref(), &left.3).cmp(&(
            &right.0,
            &right.1,
            right.2.as_ref(),
            &right.3,
        ))
    });
}

fn expected(rows: Vec<Outcome>) -> Vec<Outcome> {
    let mut rows = rows;
    sort(&mut rows);
    rows
}

/// Under `antora.yml`, a resource ID is anchored at the family directory of
/// its module rather than beside the document: a page for an xref, the named
/// family for a `$` coordinate, and the image family for an image. A page
/// under a subdirectory still reaches the family root, a component
/// coordinate stays outside this tree, and a document outside the component
/// keeps the relative reading.
#[test]
fn an_antora_component_anchors_resource_ids_at_the_family_directory() {
    let chain = amiss_fixtures::antora_component().expect("the fixture stages");
    let index = "docs/modules/api/pages/index.adoc";
    let nav = "docs/modules/api/nav.adoc";
    let want = expected(vec![
        row(nav, Some(index), ResolutionTag::Resolved, Some(index)),
        row(
            nav,
            Some("docs/modules/api/pages/absent.adoc"),
            ResolutionTag::Missing,
            Some("docs/modules/api/pages/absent.adoc"),
        ),
        row(
            index,
            Some("docs/modules/api/partials/intro.adoc"),
            ResolutionTag::Resolved,
            Some("docs/modules/api/partials/intro.adoc"),
        ),
        row(
            index,
            Some("docs/modules/api/examples/absent.rb"),
            ResolutionTag::Missing,
            Some("docs/modules/api/examples/absent.rb"),
        ),
        row(
            index,
            Some("docs/modules/install/pages/steps.adoc"),
            ResolutionTag::Resolved,
            Some("docs/modules/install/pages/steps.adoc"),
        ),
        row(
            index,
            Some("docs/modules/install/pages/absent.adoc"),
            ResolutionTag::Missing,
            Some("docs/modules/install/pages/absent.adoc"),
        ),
        row(
            index,
            Some("docs/modules/api/images/diagram.png"),
            ResolutionTag::Resolved,
            Some("docs/modules/api/images/diagram.png"),
        ),
        row(
            index,
            Some("docs/modules/api/images/absent.png"),
            ResolutionTag::Missing,
            Some("docs/modules/api/images/absent.png"),
        ),
        row(index, None, ResolutionTag::External, None),
        row(
            "docs/modules/api/pages/sub/deep.adoc",
            Some(index),
            ResolutionTag::Resolved,
            Some(index),
        ),
        row(
            "guide/notes.adoc",
            Some("guide/index.adoc"),
            ResolutionTag::Missing,
            Some("guide/index.adoc"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under a Next.js configuration, a page is `page.mdx` in the directory of
/// its route, served with no trailing slash, so a link in one reads from the
/// directory above the page's own and names the target route's `page.mdx`,
/// which a route directory without one does not serve. An image is a module
/// import and stays beside the file.
#[test]
fn a_nextjs_app_page_reads_links_from_its_route() {
    use amiss_fixtures::Staged;
    let chain = amiss_fixtures::staged_repository(&[
        ("README.md", Staged::File(b"# R\n")),
        ("site/next.config.mjs", Staged::File(b"export default {}\n")),
        ("site/app/docs/b/page.mdx", Staged::File(b"# B\n")),
        ("site/app/docs/empty/data.json", Staged::File(b"{}\n")),
        ("site/app/docs/a/img.png", Staged::File(b"png")),
        (
            "site/app/docs/a/page.mdx",
            Staged::File(b"# A\n\n[b](./b)\n\n[empty](./empty)\n\n![i](./img.png)\n"),
        ),
    ])
    .expect("the fixture stages");
    let page = "site/app/docs/a/page.mdx";
    let want = expected(vec![
        row(
            page,
            Some("site/app/docs/b/page.mdx"),
            ResolutionTag::Resolved,
            Some("site/app/docs/b/page.mdx"),
        ),
        row(
            page,
            Some("site/app/docs/empty/page.mdx"),
            ResolutionTag::Missing,
            Some("site/app/docs/empty/page.mdx"),
        ),
        row(
            page,
            Some("site/app/docs/a/img.png"),
            ResolutionTag::Resolved,
            Some("site/app/docs/a/img.png"),
        ),
    ]);
    let got: Vec<Outcome> = outcomes(&chain)
        .into_iter()
        .filter(|outcome| outcome.0 == page)
        .collect();
    assert_eq!(got, want);
}

/// A component is assembled from every source root whose `antora.yml` spells
/// its name, so a module coordinate is answered by whichever of them holds
/// the resource while the finding still names the root the author wrote
/// under. A module of the component next door answers nothing, and a
/// component whose descriptor reserves the `ext` block is assembled by an
/// extension, so a resource it does not hold is undecided rather than absent.
#[test]
fn an_antora_component_reaches_every_root_that_names_it() {
    let chain = amiss_fixtures::antora_component_roots().expect("the fixture stages");
    let index = "docs/modules/api/pages/index.adoc";
    let built = "built/modules/guide/pages/index.adoc";
    let want = expected(vec![
        row(
            index,
            Some("docs/modules/plugin/pages/build.adoc"),
            ResolutionTag::Resolved,
            Some("plugin/modules/plugin/pages/build.adoc"),
        ),
        row(
            index,
            Some("docs/modules/plugin/pages/absent.adoc"),
            ResolutionTag::Missing,
            Some("docs/modules/plugin/pages/absent.adoc"),
        ),
        row(
            index,
            Some("docs/modules/extra/pages/notes.adoc"),
            ResolutionTag::Missing,
            Some("docs/modules/extra/pages/notes.adoc"),
        ),
        row(
            built,
            Some("built/modules/guide/pages/here.adoc"),
            ResolutionTag::Resolved,
            Some("built/modules/guide/pages/here.adoc"),
        ),
        row(
            built,
            Some("built/modules/guide/pages/absent.adoc"),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under `docusaurus.config.*`, a bare Markdown destination the document's
/// own directory does not hold is asked under the content root and then the
/// site directory, `@site/` names a path from the site directory for links
/// and images alike, a `./` destination stays beside the document, and a URL
/// is external whatever its last segment spells. A destination that is a
/// route rather than a path reaches the document published there: the
/// identity a frontmatter `id` declares, the site-absolute `slug` another
/// declares, and the path of a document declaring neither, while a route
/// nothing publishes stays missing. The
/// intent keeps the spelling the author wrote while the resolution names the
/// file that answered, and a document outside the site keeps the bare path it
/// wrote while the alias names the one site this tree declares. A
/// webpack inline request is a path under no site at all, while a name that
/// carries a bang past its opening, or a directory opening with an at sign,
/// is a path like any other.
#[test]
fn a_docusaurus_site_answers_bare_paths_from_its_content_root_and_the_site_alias() {
    let chain = amiss_fixtures::docusaurus_site().expect("the fixture stages");
    let page = "website/docs/api/themes/configuration.mdx";
    let beside = "website/docs/api/themes/static-assets.mdx";
    let versioned = "website/versioned_docs/version-1.0/api/themes/configuration.mdx";
    let want = expected(vec![
        row(
            page,
            Some(beside),
            ResolutionTag::Resolved,
            Some("website/docs/static-assets.mdx"),
        ),
        row(
            page,
            Some("website/docs/api/themes/absent.mdx"),
            ResolutionTag::Missing,
            Some("website/docs/api/themes/absent.mdx"),
        ),
        row(
            page,
            Some("website/docs/api/themes/root-note.md"),
            ResolutionTag::Resolved,
            Some("website/root-note.md"),
        ),
        row(
            page,
            Some("website/static/img/logo.png"),
            ResolutionTag::Resolved,
            Some("website/static/img/logo.png"),
        ),
        row(
            page,
            Some("website/static/img/gone.png"),
            ResolutionTag::Missing,
            Some("website/static/img/gone.png"),
        ),
        row(
            page,
            Some("website/static/img/logo.png"),
            ResolutionTag::Resolved,
            Some("website/static/img/logo.png"),
        ),
        row(page, Some(beside), ResolutionTag::Missing, Some(beside)),
        row(page, None, ResolutionTag::External, None),
        row(
            page,
            Some("website/docs/api/themes/cli"),
            ResolutionTag::Resolved,
            Some("website/docs/api/themes/command-line.mdx"),
        ),
        row(
            page,
            Some("website/docs/api/themes/absent-id"),
            ResolutionTag::Missing,
            Some("website/docs/api/themes/absent-id"),
        ),
        row(
            page,
            Some("website/docs/guides/setup"),
            ResolutionTag::Resolved,
            Some("website/docs/setup.mdx"),
        ),
        row(
            page,
            Some("website/docs/api/themes/notes"),
            ResolutionTag::Resolved,
            Some("website/docs/api/themes/notes.mdx"),
        ),
        row(
            versioned,
            Some("website/versioned_docs/version-1.0/api/themes/static-assets.mdx"),
            ResolutionTag::Resolved,
            Some("website/versioned_docs/version-1.0/static-assets.mdx"),
        ),
        row(
            "README.md",
            Some("static-assets.mdx"),
            ResolutionTag::Missing,
            Some("static-assets.mdx"),
        ),
        row(
            "README.md",
            Some("website/static/img/logo.png"),
            ResolutionTag::Resolved,
            Some("website/static/img/logo.png"),
        ),
        row("README.md", None, ResolutionTag::Invalid, None),
        row(
            "README.md",
            Some("weird!name.md"),
            ResolutionTag::Resolved,
            Some("weird!name.md"),
        ),
        row(
            "README.md",
            Some("@internal/notes.md"),
            ResolutionTag::Resolved,
            Some("@internal/notes.md"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// A site under `website/` reads the sibling `docs/` its configuration names,
/// and that tree carries one site, so the page there is bound to it: the bare
/// Markdown path reaches the content root, the alias reaches the site
/// directory, and a path nothing holds is missing under the name its author
/// wrote. A rule that answers for a build rather than for the tree does not
/// bind the same way. `notes/readme.md` sits under no Jekyll declaration, so
/// the file it names stays missing rather than becoming that site's to serve.
#[test]
fn a_docusaurus_site_answers_the_sibling_docs_tree_its_configuration_reads() {
    let chain = amiss_fixtures::docusaurus_sibling_docs().expect("the fixture stages");
    let page = "docs/guides/setup.md";
    let want = expected(vec![
        row(
            page,
            Some("docs/guides/reference.md"),
            ResolutionTag::Resolved,
            Some("docs/reference.md"),
        ),
        row(
            page,
            Some("docs/guides/absent.md"),
            ResolutionTag::Missing,
            Some("docs/guides/absent.md"),
        ),
        row(
            page,
            Some("website/static/img/logo.png"),
            ResolutionTag::Resolved,
            Some("website/static/img/logo.png"),
        ),
        row(
            "notes/readme.md",
            Some("notes/absent.md"),
            ResolutionTag::Missing,
            Some("notes/absent.md"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// A document Docusaurus publishes no page for is rendered into whichever
/// pages import it, so a fragment-only link written in it names an identity
/// of a page the tree does not fix. The identity the file writes itself still
/// resolves and the one it does not is undecided rather than absent. Only the
/// fragment moves: the paths beside it are read from the file as written,
/// wherever it is rendered. The page that imports it keeps proving absence,
/// and the copy under `notes/`, with no `docusaurus.config.*` above it,
/// answers every fragment from its own text.
#[test]
fn a_docusaurus_partial_leaves_the_page_that_renders_it_undecided() {
    let chain = amiss_fixtures::docusaurus_partial().expect("the fixture stages");
    let partial = "website/docs/api/plugins/_tags.mdx";
    let page = "website/docs/api/plugins/plugin-content-docs.mdx";
    let guide = "website/docs/api/plugins/guide.mdx";
    let absent = "website/docs/api/plugins/absent.mdx";
    let outside = "notes/_tags.mdx";
    let want = expected(vec![
        row(
            partial,
            Some(partial),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
        row(
            partial,
            Some(partial),
            ResolutionTag::Resolved,
            Some(partial),
        ),
        row(partial, Some(guide), ResolutionTag::Resolved, Some(guide)),
        row(partial, Some(absent), ResolutionTag::Missing, Some(absent)),
        row(page, Some(page), ResolutionTag::Resolved, Some(page)),
        row(page, Some(page), ResolutionTag::Missing, None),
        row(outside, Some(outside), ResolutionTag::Missing, None),
        row(
            outside,
            Some(outside),
            ResolutionTag::Resolved,
            Some(outside),
        ),
        row(
            outside,
            Some("notes/guide.mdx"),
            ResolutionTag::Resolved,
            Some("notes/guide.mdx"),
        ),
        row(
            outside,
            Some("notes/absent.mdx"),
            ResolutionTag::Missing,
            Some("notes/absent.mdx"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under `mkdocs.yml`, a destination written as raw HTML is relative to the
/// directory the page is published at rather than to the source file, and the
/// trailing slash it carries names a page: the index page's own directory
/// answers a directory URL with the page source, a nested page's image climbs
/// out of the directory its own name opened, a markdown link keeps the
/// source-relative reading the generator rewrites, and a document with no
/// `mkdocs.yml` above it keeps the directory it promised. Every one of them
/// is named by the reading from the document's own directory, so no finding
/// carries the published directory the rule anchored at. An expression the
/// build fills in is not a path at all, while a file whose own name carries a
/// brace is, and a frontmatter identity is a route no mkdocs tree publishes.
#[test]
fn a_mkdocs_site_reads_a_raw_html_destination_from_the_published_directory() {
    let chain = amiss_fixtures::mkdocs_site().expect("the fixture stages");
    let index = "site/docs/index.md";
    let start = "site/docs/getting-started.md";
    let themes = "site/docs/user-guide/choosing-your-theme.md";
    let want = expected(vec![
        row(
            index,
            Some("site/docs/getting-started"),
            ResolutionTag::Resolved,
            Some(start),
        ),
        row(
            index,
            Some("site/docs/absent"),
            ResolutionTag::Missing,
            Some("site/docs/absent"),
        ),
        row(index, Some(start), ResolutionTag::Resolved, Some(start)),
        row(
            index,
            Some("site/docs/named-page"),
            ResolutionTag::Missing,
            Some("site/docs/named-page"),
        ),
        row(
            index,
            Some("site/docs/a{b}.md"),
            ResolutionTag::Resolved,
            Some("site/docs/a{b}.md"),
        ),
        row(
            themes,
            Some("site/img/light.png"),
            ResolutionTag::Resolved,
            Some("site/docs/img/light.png"),
        ),
        row(
            themes,
            Some("site/img/gone.png"),
            ResolutionTag::Missing,
            Some("site/img/gone.png"),
        ),
        row(
            themes,
            Some("site/docs/user-guide/absent"),
            ResolutionTag::Missing,
            Some("site/docs/user-guide/absent"),
        ),
        row(themes, None, ResolutionTag::UnsupportedSemantics, None),
        row(
            "notes/index.md",
            Some("notes/getting-started"),
            ResolutionTag::Missing,
            Some("notes/getting-started"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// An `AsciiDoc` document publishes what a cross reference can name: the
/// identity a block or inline anchor declares, and the reference text a
/// section title carries where a natural reference could reach it. A title no
/// document holds and an anchor nothing declares are both missing, and an
/// indented paragraph is literal text, so the reference spelled inside it is
/// never read at all.
#[test]
fn an_asciidoc_document_publishes_the_identities_a_cross_reference_names() {
    let chain = amiss_fixtures::staged_repository(&amiss_fixtures::ASCIIDOC_IDENTITIES)
        .expect("the fixture stages");
    let inline = "docs/inline.adoc";
    let literal = "docs/literal.adoc";
    let title = "docs/title.adoc";
    let want = expected(vec![
        row(inline, Some(inline), ResolutionTag::Resolved, Some(inline)),
        row(inline, Some(inline), ResolutionTag::Missing, None),
        row(
            literal,
            Some(literal),
            ResolutionTag::Resolved,
            Some(literal),
        ),
        row(title, Some(title), ResolutionTag::Resolved, Some(title)),
        row(title, Some(title), ResolutionTag::Missing, None),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under `conf.py`, a source-root-absolute `:doc:` target is the docname
/// under that directory with the source suffix, from any depth, while a
/// relative target resolves beside its document. A dot inside a relative
/// docname is part of the name, so `arrays.scalars` reaches the file that
/// name takes the suffix of, and the one the tree does not hold is reported
/// under that same spelling. A target opening with another project's name is
/// an intersphinx inventory rather than a path here. A document with no
/// `conf.py` above it is answered by the one source directory the tree
/// declares, which is the docname the author wrote it for.
#[test]
fn a_sphinx_source_directory_anchors_absolute_doc_roles_at_its_conf() {
    let chain = amiss_fixtures::sphinx_source().expect("the fixture stages");
    let want = expected(vec![
        row(
            "docs/index.rst",
            Some("docs/testing.rst"),
            ResolutionTag::Resolved,
            Some("docs/testing.rst"),
        ),
        row(
            "docs/index.rst",
            Some("docs/arrays.scalars.rst"),
            ResolutionTag::Resolved,
            Some("docs/arrays.scalars.rst"),
        ),
        row(
            "docs/index.rst",
            Some("docs/arrays.absent.rst"),
            ResolutionTag::Missing,
            Some("docs/arrays.absent.rst"),
        ),
        row("docs/index.rst", None, ResolutionTag::External, None),
        row(
            "docs/index.rst",
            Some("docs/absent.rst"),
            ResolutionTag::Missing,
            Some("docs/absent.rst"),
        ),
        row(
            "docs/index.rst",
            Some("docs/deploying/index.rst"),
            ResolutionTag::Resolved,
            Some("docs/deploying/index.rst"),
        ),
        row(
            "docs/deploying/index.rst",
            Some("docs/testing.rst"),
            ResolutionTag::Resolved,
            Some("docs/testing.rst"),
        ),
        row(
            "docs/deploying/index.rst",
            Some("docs/testing.rst"),
            ResolutionTag::Resolved,
            Some("docs/testing.rst"),
        ),
        row(
            "notes/readme.rst",
            Some("docs/testing.rst"),
            ResolutionTag::Resolved,
            Some("docs/testing.rst"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Where `conf.py` says which suffix it reads, a docname takes that suffix
/// rather than the default: `/testing` is `docs/testing.txt`. A docname
/// carrying a dot of its own is still a docname, a trailing slash is
/// normalized away before the name is looked up, and a path into a text file
/// outside the root reaches the file it names. A relative docname keeps the
/// `.rst` the adapter spelled it with, which is the stated boundary.
#[test]
fn a_declared_source_suffix_spells_the_docname_a_doc_role_names() {
    let chain = amiss_fixtures::sphinx_declared_suffix().expect("the fixture stages");
    let index = "docs/index.txt";
    let testing = "docs/testing.txt";
    let want = expected(vec![
        row(index, Some(testing), ResolutionTag::Resolved, Some(testing)),
        row(index, Some(testing), ResolutionTag::Resolved, Some(testing)),
        row(
            index,
            Some("docs/releases/1.1.txt"),
            ResolutionTag::Resolved,
            Some("docs/releases/1.1.txt"),
        ),
        row(
            index,
            Some("docs/absent.txt"),
            ResolutionTag::Missing,
            Some("docs/absent.txt"),
        ),
        row(
            index,
            Some("docs/testing.rst"),
            ResolutionTag::Missing,
            Some("docs/testing.rst"),
        ),
        row(
            index,
            Some("notes/plan.txt"),
            ResolutionTag::Resolved,
            Some("notes/plan.txt"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under a generator whose URL model this engine does not implement, a
/// relative destination the tree does not hold is the build's answer rather
/// than a missing file, while a destination the tree does hold still
/// resolves, an anchor into it is still read, and a document outside the site
/// keeps its missing path. A destination naming a page source is out of the
/// build's reach and stays the missing file it is, while the asset beside it
/// takes the boundary the whole site takes. The three other rules of this
/// kind differ only by the file that declares them.
#[test]
fn a_hugo_site_leaves_an_unresolved_relative_destination_to_its_build() {
    let chain = amiss_fixtures::hugo_site().expect("the fixture stages");
    let guide = "site/content/en/guide.md";
    let install = "site/content/en/install.md";
    let want = expected(vec![
        row(
            guide,
            Some("site/content/en/g"),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
        row(guide, Some(install), ResolutionTag::Resolved, Some(install)),
        row(guide, Some(install), ResolutionTag::Missing, None),
        row(
            guide,
            Some("site/content/en/absent.md"),
            ResolutionTag::Missing,
            Some("site/content/en/absent.md"),
        ),
        row(
            guide,
            Some("site/content/en/diagram.svg"),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
        row(
            "notes/readme.md",
            Some("notes/absent.md"),
            ResolutionTag::Missing,
            Some("notes/absent.md"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under `book.toml`, a page is served at its path under the book's source
/// directory, so a destination climbing past the book root is read back under
/// the source directory of the book that holds the page it names. That
/// directory is `src` unless the book's own `[book] src` names another. A built
/// page no book answers belongs to the site around it, a climb past the
/// outermost root reaches the same answer, and a source destination the tree
/// lacks is a missing file as before.
#[test]
fn an_mdbook_page_climbs_out_of_its_book_the_way_its_url_does() {
    let chain = amiss_fixtures::mdbook_site().expect("the fixture stages");
    let old = "second/src/ch01.md";
    let want = expected(vec![
        row(
            "src/ch01.md",
            Some("std/index.html"),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
        row(
            "src/ch01.md",
            Some("src/ch02.md"),
            ResolutionTag::Resolved,
            Some("src/ch02.md"),
        ),
        row(
            old,
            Some("second/ch01.html"),
            ResolutionTag::Resolved,
            Some("src/ch01.md"),
        ),
        row(
            old,
            Some("second/ch09.html"),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
        row(
            old,
            Some("second/src/ch07.md"),
            ResolutionTag::Missing,
            Some("second/src/ch07.md"),
        ),
        row(
            "third/chapters/intro.md",
            Some("third/ch01.html"),
            ResolutionTag::Resolved,
            Some("src/ch01.md"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under a `config.toml` beside a `content` directory, the `@/` prefix opens
/// a path from that directory, and what it reaches there is an ordinary
/// lookup: a page that exists resolves and one that does not is missing at
/// the content root. A colocated asset beside a page stays the missing file
/// it is, and a `config.toml` with no content directory beside it anchors
/// nothing.
#[test]
fn a_zola_site_anchors_the_content_root_prefix_at_its_content_directory() {
    let chain = amiss_fixtures::zola_site().expect("the fixture stages");
    let overview = "docs/content/documentation/overview.md";
    let page = "docs/content/documentation/page.md";
    let want = expected(vec![
        row(overview, Some(page), ResolutionTag::Resolved, Some(page)),
        row(
            overview,
            Some("docs/content/documentation/absent.md"),
            ResolutionTag::Missing,
            Some("docs/content/documentation/absent.md"),
        ),
        row(
            "docs/content/themes/persona/index.md",
            Some("docs/content/themes/persona/pagespeed-report.svg"),
            ResolutionTag::Missing,
            Some("docs/content/themes/persona/pagespeed-report.svg"),
        ),
        row(
            ".cargo/notes.md",
            Some(".cargo/@/documentation/page.md"),
            ResolutionTag::Missing,
            Some(".cargo/@/documentation/page.md"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// The two spellings that answer with a boundary instead of a file are out of
/// a declaration's reach, so the routers that serve nothing else are rules no
/// file a repository writes can turn on.
#[test]
fn a_declaration_reaches_no_spelling_that_withholds_an_answer() {
    let withholding = [Spelling::BuiltPage, Spelling::BuiltRoute];
    for spelling in withholding {
        assert!(
            !DECLARABLE.contains(&spelling),
            "{spelling:?} withholds an answer"
        );
    }
    let unreachable: Vec<&str> = ROUTERS
        .iter()
        .filter(|rule| {
            rule.serves
                .iter()
                .all(|spelling| withholding.contains(spelling))
        })
        .map(|rule| rule.name)
        .collect();
    assert_eq!(unreachable, ["astro", "eleventy", "hugo", "jekyll"]);
}

/// A page that moved leaves the URL it was served at in its own frontmatter,
/// and under a declared `directory-pages` the destination a reader writes for
/// that URL reaches the file holding the content now. The block reaches a file the
/// tree holds and nothing else: a URL two pages claim is decided between
/// neither, a URL no page claims is missing where it always was, and a flow
/// sequence is a shape this reader declines rather than guesses at. The
/// bundle reader's own URL is the directory holding it, so its climb is a URL
/// the block answers, while the leaf reader beside it writes the same
/// destination and the reading beside its source is no URL, so it stays
/// missing. Under `plain/`, where nothing declares a router, the same block
/// and the same destination leave the same missing path they always left.
#[test]
fn a_page_that_moved_answers_the_url_it_declares_it_moved_from() {
    let chain = amiss_fixtures::page_redirects().expect("the fixture stages");
    let notes = "site/notes/index.md";
    let want: Vec<Outcome> = [
        (
            notes,
            "site/old/guide",
            ResolutionTag::Resolved,
            Some("site/guide/index.md"),
        ),
        (
            notes,
            "site/shared/page",
            ResolutionTag::Missing,
            Some("site/shared/page"),
        ),
        (
            notes,
            "site/nothing",
            ResolutionTag::Missing,
            Some("site/nothing"),
        ),
        (
            notes,
            "site/flowed",
            ResolutionTag::Missing,
            Some("site/flowed"),
        ),
        (
            "site/notes/reader.md",
            "site/old/guide",
            ResolutionTag::Missing,
            Some("site/old/guide"),
        ),
        (
            "plain/index.md",
            "plain/old/guide",
            ResolutionTag::Missing,
            Some("plain/old/guide"),
        ),
    ]
    .into_iter()
    .map(|(document, intent, tag, answered)| row(document, Some(intent), tag, answered))
    .collect();
    assert_eq!(outcomes(&chain), expected(want));
}

/// A declaration that says where its directory is published reads a site
/// route as a path under that directory. The base opens the route and the
/// rest is the path, so `/manual/guide/` is the source `docs/guide.md` the
/// extensionless spelling serves, and `/manual/intro/` is the directory the
/// tree holds. A route the tree answers with nothing keeps the boundary it
/// had, since the build serves routes no tree holds, and so does a route
/// outside the base and a document no declaration covers. A fragment is read
/// once its path resolves, which is the claim this reading can add.
#[test]
fn a_declared_base_reads_a_site_route_as_a_path_under_the_directory() {
    let chain = amiss_fixtures::declared_site_base().expect("the fixture stages");
    let page = "docs/page.md";
    let want: Vec<Outcome> = vec![
        row(
            page,
            Some("docs/guide"),
            ResolutionTag::Resolved,
            Some("docs/guide.md"),
        ),
        row(page, Some("docs/guide"), ResolutionTag::Missing, None),
        row(page, Some("docs/intro"), ResolutionTag::Resolved, None),
        row(page, None, ResolutionTag::UnsupportedSemantics, None),
        row(page, None, ResolutionTag::UnsupportedSemantics, None),
        row(
            "outside/page.md",
            None,
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
    ];
    assert_eq!(outcomes(&chain), expected(want));
}

/// A site whose own configuration says where its pages sit and where it is
/// served needs no declaration to say it again. `contentDir` and `baseURL`
/// read `/handbook/guide/` as `site/content/en/guide.md`, from a page of any
/// language of the site rather than only from one beneath that root, since a
/// root belongs to its project. A language table names a root of its own,
/// served under its code, so `/handbook/fr/guide/` is the French page and the
/// route without that code stays the default language's. A route the tree
/// answers with nothing keeps the boundary it had, and so does one outside
/// the base.
#[test]
fn a_configuration_that_names_its_own_content_root_reads_a_site_route() {
    let chain = amiss_fixtures::configured_site().expect("the fixture stages");
    let page = "site/content/en/page.md";
    let guide = "site/content/en/guide.md";
    let want: Vec<Outcome> = vec![
        row(
            page,
            Some("site/content/en/guide"),
            ResolutionTag::Resolved,
            Some(guide),
        ),
        row(page, None, ResolutionTag::UnsupportedSemantics, None),
        row(page, None, ResolutionTag::UnsupportedSemantics, None),
        row(
            page,
            Some("site/content/en/guide"),
            ResolutionTag::Missing,
            None,
        ),
        row(
            "site/content/fr/page.md",
            Some("site/content/en/guide"),
            ResolutionTag::Resolved,
            Some(guide),
        ),
        row(
            "site/content/fr/page.md",
            Some("site/content/fr/guide"),
            ResolutionTag::Resolved,
            Some("site/content/fr/guide.md"),
        ),
    ];
    assert_eq!(outcomes(&chain), expected(want));
}

/// A configuration that mounts its content rather than naming a directory
/// still says where the pages are. The one mount targeting the content
/// directory names the root, the mount beside it names nothing, and a route
/// the tree answers with nothing keeps the boundary it had.
#[test]
fn a_module_mount_names_the_content_root_a_directory_key_would_have() {
    let chain = amiss_fixtures::mounted_site().expect("the fixture stages");
    let page = "site/pages/page.md";
    let want: Vec<Outcome> = vec![
        row(
            page,
            Some("site/pages/guide"),
            ResolutionTag::Resolved,
            Some("site/pages/guide.md"),
        ),
        row(page, None, ResolutionTag::UnsupportedSemantics, None),
    ];
    assert_eq!(outcomes(&chain), expected(want));
}

/// A Docusaurus site reads an HTML comment the MDX grammar refuses, so the
/// page it holds is read and a link inside the comment is not. The same page
/// in a tree no site holds stays refused and contributes nothing.
#[test]
fn a_docusaurus_site_reads_the_comment_mdx_refuses() {
    let chain = amiss_fixtures::staged_repository(&amiss_fixtures::DOCUSAURUS_COMMENTS)
        .expect("the fixture stages");
    let want: Vec<Outcome> = vec![row(
        "site/docs/page.mdx",
        Some("site/docs/guide.md"),
        ResolutionTag::Resolved,
        Some("site/docs/guide.md"),
    )];
    assert_eq!(outcomes(&chain), expected(want));
}

/// A configuration file under a name Hugo shares is Hugo's where it binds
/// `baseURL` beside the content it reads, and a file under `config/_default`
/// is Hugo's by where it sits. There a relative link only a built page answers
/// is the build's to answer. A `baseURL` with no content beside it, and a
/// file binding no address, configure no site, so the same link is missing.
#[test]
fn a_shared_configuration_name_is_hugo_only_where_its_bindings_say_so() {
    let chain = amiss_fixtures::staged_repository(&amiss_fixtures::HUGO_CONFIG_SPELLINGS)
        .expect("the fixture stages");
    let want: Vec<Outcome> = vec![
        row(
            "site/content/docs/page.md",
            Some("site/content/b"),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
        row(
            "nested/content/docs/page.md",
            Some("nested/content/b"),
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
        row(
            "tool/docs/page.md",
            Some("tool/b"),
            ResolutionTag::Missing,
            Some("tool/b"),
        ),
        row(
            "plain/content/docs/page.md",
            Some("plain/content/b"),
            ResolutionTag::Missing,
            Some("plain/content/b"),
        ),
    ];
    assert_eq!(outcomes(&chain), expected(want));
}

/// The page-URL reading answers to one name. `directory-pages` says the site
/// publishes a page at a directory of its own name, whatever generator builds
/// it, and the generator name that used to say it declares no rule, so the
/// identical destination beside it stays the missing path the author wrote.
#[test]
fn the_page_url_reading_answers_to_the_behaviour_name_alone() {
    let chain = amiss_fixtures::declared_page_router().expect("the fixture stages");
    let want: Vec<Outcome> = vec![
        row(
            "named/setup/live.md",
            Some("named/guide"),
            ResolutionTag::Resolved,
            Some("named/setup/guide/index.md"),
        ),
        row(
            "retired/setup/live.md",
            Some("retired/guide"),
            ResolutionTag::Missing,
            Some("retired/guide"),
        ),
    ];
    assert_eq!(outcomes(&chain), expected(want));
}

/// A repository that declares its own router gets that router's resolving
/// spellings where no configuration file names the generator. Under
/// `directory-pages` a page is published at a directory of its own name, so a
/// relative destination is read from that URL as well as from the source
/// directory: the climb out of `setup/live/` reaches the branch bundle and
/// the leaf bundle, while the destination beside the source still answers and
/// the one nothing holds is still missing under the path the author wrote. An
/// anchor the reached page does not publish is still a missing anchor, so the
/// declaration adds an answer and withholds none. The branch bundle's own
/// page is its directory, so its climb passes `setup` rather than reaching
/// the page beside it, and a declaration naming a router that only withholds
/// leaves its own tree exactly as it was.
#[test]
fn a_declared_router_reads_a_destination_from_the_page_it_publishes() {
    let chain = amiss_fixtures::declared_router().expect("the fixture stages");
    let live = "site/setup/live.md";
    let config = "site/setup/config/_index.md";
    let sibling = "site/setup/sibling.md";
    let want: Vec<Outcome> = [
        (live, "site/config", ResolutionTag::Resolved, Some(config)),
        (live, "site/config", ResolutionTag::Missing, None),
        (
            live,
            "site/nothing",
            ResolutionTag::Missing,
            Some("site/nothing"),
        ),
        (live, sibling, ResolutionTag::Resolved, Some(sibling)),
        (
            live,
            "site/guide",
            ResolutionTag::Resolved,
            Some("site/setup/guide/index.md"),
        ),
        (
            config,
            "site/setup/trap",
            ResolutionTag::Missing,
            Some("site/setup/trap"),
        ),
        (
            "other/page.md",
            "other/absent",
            ResolutionTag::Missing,
            Some("other/absent"),
        ),
    ]
    .into_iter()
    .map(|(document, intent, tag, answered)| row(document, Some(intent), tag, answered))
    .collect();
    let served = outcomes(&chain);
    for wanted in &want {
        assert!(served.contains(wanted), "{wanted:?} is one of {served:?}");
    }
    assert_eq!(served.len(), want.len(), "{served:?}");
}
