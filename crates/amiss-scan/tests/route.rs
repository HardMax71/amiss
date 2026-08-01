#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over the published route-spelling vectors"
)]

amiss_fixtures::bounded_memory!();

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use amiss_scan::route::{ROUTERS, RouteRule, Spelling, candidates, spellings};
use amiss_wire::json::{Value, parse};
use amiss_wire::model::RepoPath;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectors() -> Value {
    let bytes = fs::read(root().join("spec/examples/route-spelling-vectors.json"))
        .expect("the specification ships the route-spelling vectors");
    parse(&bytes).expect("route-spelling vectors are strict JSON")
}

fn member(value: &Value, key: &str, label: &str) -> Value {
    let Value::Object(members) = value else {
        panic!("{label} is an object")
    };
    members.iter().find(|(name, _)| name == key).map_or_else(
        || panic!("{label} has no {key}"),
        |(_, found)| found.clone(),
    )
}

fn text(value: &Value, key: &str, label: &str) -> String {
    let found = member(value, key, label);
    let Value::String(found) = found else {
        panic!("{label}.{key} is a string, found {found:?}")
    };
    found
}

fn array(value: &Value, key: &str, label: &str) -> Vec<Value> {
    let found = member(value, key, label);
    let Value::Array(found) = found else {
        panic!("{label}.{key} is an array, found {found:?}")
    };
    found
}

fn path(raw: &str) -> RepoPath {
    RepoPath::new(raw.to_owned()).unwrap_or_else(|| panic!("{raw} is a repository path"))
}

fn tree(vectors: &Value) -> BTreeSet<String> {
    array(vectors, "tree", "vectors")
        .into_iter()
        .map(|entry| {
            let Value::String(entry) = entry else {
                panic!("a tree entry is a string")
            };
            entry
        })
        .collect()
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
        text(&vectors, "schema", "vectors"),
        "amiss/route-spelling-vectors"
    );
    assert_eq!(text(&vectors, "contract", "vectors"), "route-spelling");

    let tree = tree(&vectors);
    let cases = array(&vectors, "cases", "vectors");
    assert!(cases.len() >= 14, "the corpus keeps its probe set");
    let mut seen = BTreeSet::new();
    let mut asked = 0_usize;
    for case in &cases {
        let id = text(case, "id", "case");
        assert!(seen.insert(id.clone()), "case {id} is unique");
        let destination = text(case, "destination", "case");
        if answered_by_the_tree(&destination, &tree) {
            continue;
        }
        asked = asked.saturating_add(1);
        let harvest = member(case, "serves", "case");
        for rule in &ROUTERS {
            let found = member(&harvest, rule.name, &format!("case {id}"));
            let want = if let Value::String(source) = found {
                Some(source)
            } else if found == Value::Null {
                None
            } else {
                panic!(
                    "case {id}.serves.{} is a string or null, got {found:?}",
                    rule.name
                )
            };
            assert_eq!(
                served(rule, &destination, &tree),
                want,
                "case {id} under {}: destination {destination:?}",
                rule.name
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

/// mkdocs demands the source spelling, which is why a repository it serves
/// gains nothing here and loses nothing either.
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
