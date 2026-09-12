#![expect(
    clippy::expect_used,
    reason = "integration assertions over the published heading-anchor vectors"
)]

use amiss_md::HeadingSource;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use amiss_md::{Heading, analyze};
use amiss_scan::anchor::{Attribute, RULES, RawHtml, anchor_set, identities};
use amiss_wire::model::Adapter;
use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectors() -> Value {
    let bytes = fs::read(root().join("spec/examples/heading-anchor-vectors.json"))
        .expect("the specification ships the heading-anchor vectors");
    serde_json::from_slice::<Value>(&bytes).expect("heading-anchor vectors are JSON")
}

fn headings(source: &str) -> (Vec<Heading>, Vec<String>, Vec<String>) {
    parsed(Adapter::Markdown, source)
}

fn parsed(adapter: Adapter, source: &str) -> (Vec<Heading>, Vec<String>, Vec<String>) {
    let extraction = analyze(adapter, source.as_bytes(), u64::MAX)
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
        vectors["schema"].as_str().expect("vector schema"),
        "amiss/heading-anchor-vectors"
    );
    assert_eq!(
        vectors["contract"].as_str().expect("vector contract"),
        "heading-anchor"
    );

    let cases = vectors["cases"].as_array().expect("heading cases");
    assert!(cases.len() >= 24, "the corpus keeps its divergence cases");
    let mut seen = BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().expect("id string");
        assert!(seen.insert(id), "case {id} is unique");
        let heading = case["heading"].as_str().expect("heading string");
        let expected = case["ids"].as_object().expect("expected anchor identities");
        let headings = one(heading);
        for rule in &RULES {
            let published = identities(rule, &headings);
            let found = expected.get(rule.name).expect("every rule has a verdict");
            let want: Vec<String> = serde_json::from_value::<Option<String>>(found.clone())
                .expect("anchor identity is a string or null")
                .into_iter()
                .collect();
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
    let documents = vectors["documents"].as_array().expect("rendered documents");
    assert!(documents.len() >= 5, "the corpus keeps its rendered pairs");
    let directory = root().join("corpus/third_party/anchor-fixtures");

    for document in documents {
        let id = document["id"].as_str().expect("id string");
        let label = format!("document {id}");
        let rule_name = document["rule"].as_str().expect("rule string");
        let prefix = document["prefix"].as_str().expect("prefix string");
        let source = fs::read_to_string(
            directory.join(document["document"].as_str().expect("document string")),
        )
        .expect("the fixture document is readable");
        let published = fs::read_to_string(
            directory.join(document["identities"].as_str().expect("identities string")),
        )
        .expect("the published identities are readable");

        let want: Vec<String> = published
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| line.strip_prefix(prefix).unwrap_or(line).to_owned())
            .collect();
        let rule = RULES
            .iter()
            .find(|rule| rule.name == rule_name)
            .unwrap_or_else(|| panic!("{label} names a known rule"));
        let name = document["document"].as_str().expect("document string");
        let adapter = match name.rsplit('.').next() {
            Some("mdx") => Adapter::Mdx,
            Some(_) | None => Adapter::Markdown,
        };
        let (headings, anchors, declared) = parsed(adapter, &source);
        if document.get("covers").and_then(Value::as_str) == Some("union") {
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
            "asciidoctor" => vec!["_same", "_same_2", "_same_3"],
            "python-markdown" | "pymdownx" => vec!["same", "same_1", "same_2"],
            _ => vec!["same", "same-1", "same-2"],
        }
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(ids, want, "{name}");
    }
}

#[test]
fn generated_suffixes_skip_interleaved_authored_identities() {
    let rule = RULES
        .iter()
        .find(|rule| rule.name == "github")
        .expect("the table holds the github rule");
    let (dash_headings, _anchors, _declared) =
        headings("# Same\n\n# Same-1\n\n# Same\n\n# Same-2\n\n# Same\n");
    assert_eq!(
        identities(rule, &dash_headings),
        ["same", "same-1", "same-2", "same-2-1", "same-3"].map(str::to_owned)
    );

    let rule = RULES
        .iter()
        .find(|rule| rule.name == "python-markdown")
        .expect("the table holds the python-markdown rule");
    let (underscore_headings, _anchors, _declared) =
        headings("# Same\n\n# Same_1\n\n# Same\n\n# Same_2\n\n# Same\n");
    assert_eq!(
        identities(rule, &underscore_headings),
        ["same", "same_1", "same_2", "same_3", "same_4"].map(str::to_owned)
    );
}

/// A heading already ending in digits must not lose its tail to the
/// increment parser: the first duplicate appends a fresh counter instead of
/// bumping digits that belong to the title.
#[test]
fn a_digit_tailed_heading_keeps_its_digits_through_duplication() {
    let (headings, _anchors, _declared) = headings("# Sec2\n\n# Sec2\n\n# Sec2\n");
    for rule in &RULES {
        if !matches!(rule.name, "python-markdown" | "pymdownx") {
            continue;
        }
        assert_eq!(
            identities(rule, &headings),
            vec!["sec2".to_owned(), "sec2_1".to_owned(), "sec2_2".to_owned()],
            "{}",
            rule.name
        );
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
        } else if rule.name == "asciidoctor" {
            vec!["_twin".to_owned()]
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
            "gitea" | "docutils" => {
                assert!(published.is_empty(), "{} publishes no anchor", rule.name);
            }
            "asciidoctor" => assert_eq!(published, vec!["_...".to_owned()]),
            "forgejo" | "goldmark" => assert_eq!(published, vec!["heading".to_owned()]),
            "kramdown" => assert_eq!(published, vec!["section".to_owned()]),
            "python-markdown" | "pymdownx" => assert_eq!(published, vec!["_1".to_owned()]),
            _ => assert_eq!(published, vec![String::new()], "{}", rule.name),
        }
    }
}

/// The generated identity is what an `AsciiDoc` cross reference actually names,
/// and a document that splices another file cannot answer for one it lacks.
#[test]
fn asciidoctor_publishes_its_generated_identity() {
    let rule = RULES
        .iter()
        .find(|rule| rule.name == "asciidoctor")
        .expect("the table holds the asciidoctor rule");
    let cases = [
        ("Named Part", "_named_part"),
        ("A -- B", "_a_--_b"),
        ("a.b.c", "_a.b.c"),
        ("3D printing", "_3d_printing"),
        ("Ⅻ chapter", "__chapter"),
    ];
    for (text, want) in cases {
        let heading = Heading {
            text: text.to_owned(),
            attribute: None,
            source: HeadingSource::Markdown,
            span: (0, text.len()),
        };
        assert_eq!(
            identities(rule, &[heading]),
            vec![want.to_owned()],
            "{text}"
        );
    }
}

/// Docutils turns every non-alphanumeric run into one separator, which no other
/// rule in the table does, and strips a leading digit run rather than prefixing.
#[test]
fn docutils_publishes_the_identity_make_id_computes() {
    let rule = RULES
        .iter()
        .find(|rule| rule.name == "docutils")
        .expect("the table holds the docutils rule");
    for (text, want) in [
        ("foo_bar baz", "foo-bar-baz"),
        ("a.b.c", "a-b-c"),
        ("3D printing", "d-printing"),
        ("-hyphen-", "hyphen"),
        ("Ⅻ chapter", "xii-chapter"),
    ] {
        let heading = Heading {
            text: text.to_owned(),
            attribute: None,
            source: HeadingSource::Markdown,
            span: (0, text.len()),
        };
        assert_eq!(
            identities(rule, &[heading]),
            vec![want.to_owned()],
            "{text}"
        );
    }
}
