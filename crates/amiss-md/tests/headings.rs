use amiss_md::{Extraction, Heading, analyze};
use amiss_wire::model::Adapter;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn extraction(adapter: Adapter, source: &str) -> Extraction {
    analyze(adapter, source.as_bytes(), u64::MAX)
        .expect("analyze")
        .extraction
        .expect("a parsing adapter extracts")
}

fn texts(extraction: &Extraction) -> Vec<String> {
    extraction
        .headings
        .iter()
        .map(|heading| heading.text.clone())
        .collect()
}

fn only(extraction: &Extraction) -> Heading {
    let mut headings = extraction.headings.clone();
    assert_eq!(headings.len(), 1, "expected exactly one heading");
    headings.remove(0)
}

/// The text every renderer slugs by: literal code and link labels, with
/// emphasis, images and raw HTML contributing nothing of their own.
#[test]
fn heading_text_is_the_rendered_text_content() {
    let source = "## A *b* `c` [d](x) ![e](i.png) <b>f</b> g\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(texts(&got), vec!["A b c d  f g".to_owned()]);
}

#[test]
fn a_footnote_call_in_a_heading_contributes_nothing() {
    let source = "## Note[^1]\n\n[^1]: text\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(texts(&got), vec!["Note".to_owned()]);
}

#[test]
fn setext_and_closed_atx_headings_are_recorded() {
    let source = "Title\n=====\n\n## Closed ##\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(texts(&got), vec!["Title".to_owned(), "Closed".to_owned()]);
}

#[test]
fn headings_are_recorded_in_document_order_with_their_spans() {
    let source = "# One\n\n> ## Two\n\n- ### Three\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(
        texts(&got),
        vec!["One".to_owned(), "Two".to_owned(), "Three".to_owned()]
    );
    let spans: Vec<(usize, usize)> = got.headings.iter().map(|heading| heading.span).collect();
    assert_eq!(spans, vec![(0, 5), (9, 15), (19, 28)]);
}

/// Both attribute spellings split off, and the removed bytes stay whole, so a
/// renderer that ignores the syntax reads the text and the suffix together.
#[test]
fn both_attribute_spellings_split_off_reversibly() {
    for (source, text, id) in [
        ("## Explicit {#custom}\n", "Explicit", "custom"),
        ("## Explicit {: #custom }\n", "Explicit", "custom"),
        ("## Explicit{#custom}\n", "Explicit", "custom"),
    ] {
        let got = extraction(Adapter::Markdown, source);
        let heading = only(&got);
        assert_eq!(heading.text, text);
        let attribute = heading.attribute.clone();
        let Some(attribute) = attribute else {
            panic!("{source:?} carries an attribute");
        };
        assert_eq!(attribute.id, id);
        assert_eq!(
            format!("{}{}", heading.text, attribute.suffix),
            source.trim_start_matches("## ").trim_end()
        );
    }
}

#[test]
fn braces_that_are_not_an_identity_stay_in_the_text() {
    for source in [
        "## Set {a, b}\n",
        "## Empty {#}\n",
        "## Spaced {# two words}\n",
        "## Nested {#a{b}\n",
    ] {
        let got = extraction(Adapter::Markdown, source);
        let heading = only(&got);
        assert!(
            heading.attribute.is_none(),
            "{source:?} carries no identity"
        );
        assert!(heading.text.contains('{'), "{source:?} keeps its braces");
    }
}

#[test]
fn raw_html_publishes_its_id_and_name_attributes() {
    let source = "<a name=\"first\"></a>\n\n<h2 id='second'>x</h2>\n\n<div id=third>y</div>\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(
        got.html_anchors,
        vec!["first".to_owned(), "second".to_owned(), "third".to_owned()]
    );
}

#[test]
fn an_attribute_name_needs_its_own_word_boundary() {
    let source = "<div data-id=\"skipped\" hidden-name=\"skipped\" id=\"kept\"></div>\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(got.html_anchors, vec!["kept".to_owned()]);
}

#[test]
fn inline_html_inside_a_heading_publishes_its_anchor() {
    let source = "## <a id=\"inline\"></a>Title\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(texts(&got), vec!["Title".to_owned()]);
    assert_eq!(got.html_anchors, vec!["inline".to_owned()]);
}

#[test]
fn an_mdx_element_in_a_heading_contributes_no_text_and_no_anchor() {
    let source = "## Title <Badge id=\"x\" />\n";
    let got = extraction(Adapter::Mdx, source);
    assert_eq!(texts(&got), vec!["Title ".to_owned()]);
    assert!(got.html_anchors.is_empty());
}

#[test]
fn a_document_without_headings_records_none() {
    let got = extraction(Adapter::Markdown, "text [a](b)\n");
    assert!(got.headings.is_empty());
    assert!(got.html_anchors.is_empty());
}
