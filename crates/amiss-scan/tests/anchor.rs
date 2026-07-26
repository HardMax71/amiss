#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over the published heading-anchor vectors"
)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use amiss_md::{Heading, analyze};
use amiss_scan::anchor::{Attribute, RULES, RawHtml, anchor_set, identities};
use amiss_wire::json::{Value, parse};
use amiss_wire::model::Adapter;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectors() -> Value {
    let bytes = fs::read(root().join("spec/examples/heading-anchor-vectors.json"))
        .expect("the specification ships the heading-anchor vectors");
    parse(&bytes).expect("heading-anchor vectors are strict JSON")
}

fn members(value: &Value, label: &str) -> Vec<(String, Value)> {
    let Value::Object(members) = value else {
        panic!("{label} is an object")
    };
    members.clone()
}

fn member(value: &Value, key: &str, label: &str) -> Value {
    members(value, label)
        .into_iter()
        .find(|(name, _)| name == key)
        .map_or_else(|| panic!("{label} has no {key}"), |(_, found)| found)
}

fn optional(value: &Value, key: &str) -> Option<Value> {
    let Value::Object(members) = value else {
        return None;
    };
    members
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, found)| found.clone())
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

fn headings(source: &str) -> (Vec<Heading>, Vec<String>, Vec<String>) {
    let extraction = analyze(Adapter::Markdown, source.as_bytes(), u64::MAX)
        .expect("the fixture parses")
        .extraction
        .expect("a parsing adapter extracts");
    (
        extraction.headings,
        extraction.html_anchors,
        extraction.declared_anchors,
    )
}

fn one(text: &str) -> Vec<Heading> {
    let (headings, _anchors, _declared) = headings(&format!("## {text}\n"));
    assert_eq!(headings.len(), 1, "{text:?} is one heading");
    headings
}

/// Every case names what each renderer publishes for one heading, and every
/// value came from that renderer rather than from this repository.
#[test]
fn the_published_vectors_drive_every_rule() {
    let vectors = vectors();
    assert_eq!(
        text(&vectors, "schema", "vectors"),
        "amiss/heading-anchor-vectors"
    );
    assert_eq!(text(&vectors, "contract", "vectors"), "heading-anchor");

    let cases = array(&vectors, "cases", "vectors");
    assert!(cases.len() >= 24, "the corpus keeps its divergence cases");
    let mut seen = BTreeSet::new();
    for case in &cases {
        let id = text(case, "id", "case");
        assert!(seen.insert(id.clone()), "case {id} is unique");
        let heading = text(case, "heading", "case");
        let expected = member(case, "ids", "case");
        let headings = one(&heading);
        for rule in &RULES {
            let published = identities(rule, &headings);
            let found = member(&expected, rule.name, &format!("case {id}"));
            let want = if let Value::String(identity) = &found {
                vec![identity.clone()]
            } else if found == Value::Null {
                Vec::new()
            } else {
                panic!(
                    "case {id}.ids.{} is a string or null, got {found:?}",
                    rule.name
                )
            };
            assert_eq!(
                published, want,
                "case {id} under {}: heading {heading:?}",
                rule.name
            );
        }
    }
}

/// Each document was rendered by the renderer named beside it, so the rule is
/// compared with what that renderer published rather than with itself.
#[test]
fn every_rendered_document_reproduces_its_identities() {
    let vectors = vectors();
    let documents = array(&vectors, "documents", "vectors");
    assert!(documents.len() >= 5, "the corpus keeps its rendered pairs");
    let directory = root().join("corpus/third_party/anchor-fixtures");

    for document in &documents {
        let id = text(document, "id", "document");
        let label = format!("document {id}");
        let rule_name = text(document, "rule", &label);
        let prefix = text(document, "prefix", &label);
        let source = fs::read_to_string(directory.join(text(document, "document", &label)))
            .expect("the fixture document is readable");
        let published = fs::read_to_string(directory.join(text(document, "identities", &label)))
            .expect("the published identities are readable");

        let want: Vec<String> = published
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| line.strip_prefix(&prefix).unwrap_or(line).to_owned())
            .collect();
        let rule = RULES
            .iter()
            .find(|rule| rule.name == rule_name)
            .unwrap_or_else(|| panic!("{label} names a known rule"));
        let (headings, anchors, declared) = headings(&source);
        if optional(document, "covers") == Some(Value::String("union".to_owned())) {
            let union = anchor_set(&headings, &anchors, &declared);
            for identity in &want {
                assert!(
                    union.contains(identity),
                    "{label}: {identity} is published by {rule_name} and inside the union"
                );
            }
            continue;
        }
        assert_eq!(
            identities(rule, &headings),
            want,
            "{label}: {rule_name} reproduces what its renderer published"
        );
    }
}

#[test]
fn the_union_holds_every_rule_and_every_html_anchor() {
    let source = "## Setup & Config\n\n<a name=\"html-declared\"></a>\n\n[](){#block-declared}\n";
    let (headings, anchors, declared) = headings(source);
    let union = anchor_set(&headings, &anchors, &declared);
    assert!(union.contains("setup--config"), "the github family is in");
    assert!(
        union.contains("setup-config"),
        "the collapsing rules are in"
    );
    assert!(union.contains("html-declared"), "raw HTML anchors are in");
    assert!(
        union.contains("block-declared"),
        "attribute-block anchors are in"
    );
    for rule in &RULES {
        for identity in identities(rule, &headings) {
            assert!(
                union.contains(&identity),
                "{} is inside the union",
                rule.name
            );
        }
    }
}

#[test]
fn an_attribute_identity_replaces_the_slug_only_where_the_renderer_honours_it() {
    let (headings, _anchors, _declared) = headings("## Explicit {#custom-id}\n");
    for rule in &RULES {
        let published = identities(rule, &headings);
        let want = if rule.attribute == Attribute::Honored {
            "custom-id"
        } else if rule.name == "github" {
            "explicit-custom-id"
        } else {
            continue;
        };
        assert_eq!(published, vec![want.to_owned()], "{}", rule.name);
    }
}

#[test]
fn duplicate_headings_diverge_by_suffix_style() {
    let (headings, _anchors, _declared) = headings("# Same\n\n# Same\n\n# Same\n");
    let published: Vec<(&str, Vec<String>)> = RULES
        .iter()
        .map(|rule| (rule.name, identities(rule, &headings)))
        .collect();
    for (name, ids) in published {
        let want: Vec<String> = match name {
            "gitea" => vec!["same", "same", "same"],
            "python-markdown" | "pymdownx" => vec!["same", "same_1", "same_2"],
            _ => vec!["same", "same-1", "same-2"],
        }
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(ids, want, "{name}");
    }
}

/// Only the rules that run over rendered HTML see a heading written that way,
/// and the ones that do count it in the same duplicate sequence.
#[test]
fn a_raw_html_heading_belongs_to_the_rules_that_anchor_one() {
    let (headings, _anchors, _declared) = headings("<h2>Twin</h2>\n\n## Twin\n");
    for rule in &RULES {
        let published = identities(rule, &headings);
        let want: Vec<String> = if rule.raw_html == RawHtml::Anchored {
            vec!["twin".to_owned(), "twin-1".to_owned()]
        } else {
            vec!["twin".to_owned()]
        };
        assert_eq!(published, want, "{}", rule.name);
    }
}

#[test]
fn a_heading_that_filters_to_nothing_diverges_by_empty_rule() {
    let (headings, _anchors, _declared) = headings("## ...\n");
    for rule in &RULES {
        let published = identities(rule, &headings);
        match rule.name {
            "gitea" => assert!(published.is_empty(), "gitea publishes no anchor"),
            "forgejo" | "goldmark" => assert_eq!(published, vec!["heading".to_owned()]),
            "kramdown" => assert_eq!(published, vec!["section".to_owned()]),
            "python-markdown" | "pymdownx" => assert_eq!(published, vec!["_1".to_owned()]),
            _ => assert_eq!(published, vec![String::new()], "{}", rule.name),
        }
    }
}
