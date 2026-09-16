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
use amiss_scan::route::{ROUTERS, RouteRule, Spelling, candidates, spellings};
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::model::{Occurrence, occurrences};
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

/// The elided extension is not offered where the destination already carries
/// a source or output name, so no `page.md.md` is ever looked up.
#[test]
fn the_elided_extension_only_applies_to_a_bare_name() {
    let vitepress = ROUTERS
        .iter()
        .find(|rule| rule.name == "vitepress")
        .unwrap_or_else(|| panic!("vitepress is a known router"));
    for (destination, want) in [
        ("docs/page", Some("docs/page.md")),
        ("docs/page.markdown", None),
        ("docs/page.md", None),
    ] {
        let offered: Vec<String> = spellings(vitepress, &path(destination))
            .into_iter()
            .filter(|(spelling, _)| *spelling == Spelling::Extensionless)
            .filter_map(|(_, candidate)| candidate.as_str().map(str::to_owned))
            .collect();
        assert_eq!(offered.first().map(String::as_str), want, "{destination}");
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

fn outcome(occurrence: &Occurrence<RepoPath, Resolution<RepoPath>>) -> Outcome {
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

/// Under `docusaurus.config.*`, a bare Markdown destination the document's
/// own directory does not hold is asked under the content root and then the
/// site directory, `@site/` names a path from the site directory for links
/// and images alike, a `./` destination stays beside the document, and a URL
/// is external whatever its last segment spells. The
/// intent keeps the spelling the author wrote while the resolution names the
/// file that answered, and a document outside the site gains nothing.
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
            Some("@site/static/img/logo.png"),
            ResolutionTag::Missing,
            Some("@site/static/img/logo.png"),
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
/// `mkdocs.yml` above it keeps the directory it promised.
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
            themes,
            Some("site/docs/img/light.png"),
            ResolutionTag::Resolved,
            Some("site/docs/img/light.png"),
        ),
        row(
            themes,
            Some("site/docs/img/gone.png"),
            ResolutionTag::Missing,
            Some("site/docs/img/gone.png"),
        ),
        row(
            "notes/index.md",
            Some("notes/getting-started"),
            ResolutionTag::Missing,
            Some("notes/getting-started"),
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}

/// Under `conf.py`, a source-root-absolute `:doc:` target is the docname
/// under that directory with the source suffix, from any depth, while a
/// relative target keeps resolving beside its document and a document with no
/// `conf.py` above it keeps the declared site route.
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
            None,
            ResolutionTag::UnsupportedSemantics,
            None,
        ),
    ]);
    assert_eq!(outcomes(&chain), want);
}
