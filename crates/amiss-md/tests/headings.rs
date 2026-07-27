use amiss_md::{Extraction, Heading, HeadingSource, analyze};
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

/// Each case is a heading source, the text left after the identity is split
/// off, and the identity itself.
#[expect(clippy::panic, reason = "test fixture helper")]
fn names_the_heading(adapter: Adapter, cases: &[(&str, &str, &str)]) {
    for (source, text, id) in cases {
        let got = extraction(adapter, source);
        let heading = only(&got);
        assert_eq!(&heading.text, text, "{source:?}");
        let Some(attribute) = heading.attribute.clone() else {
            panic!("{source:?} carries an identity")
        };
        assert_eq!(&attribute.id, id, "{source:?}");
    }
}

/// `attr_list` accepts the identity in three spellings and among other items,
/// and every one of them names the heading.
#[test]
fn every_attribute_spelling_names_the_heading() {
    let cases = [
        ("## Pair { id=\"pair-id\" }\n", "Pair", "pair-id"),
        ("## Bare { id=bare-id }\n", "Bare", "bare-id"),
        ("## Classes { .cls #both-id }\n", "Classes", "both-id"),
        ("## Trailing {#first .cls}\n", "Trailing", "first"),
    ];
    names_the_heading(Adapter::Markdown, &cases);
    for (source, _text, _id) in cases {
        let got = extraction(Adapter::Markdown, source);
        let heading = only(&got);
        let suffix = heading.attribute.clone().map(|attribute| attribute.suffix);
        assert_eq!(
            format!("{}{}", heading.text, suffix.unwrap_or_default()),
            source.trim_start_matches("## ").trim_end(),
            "the removed bytes stay whole"
        );
    }
}

/// A block whose last line is an attribute block declares that identity for
/// itself. One that trails other text declares nothing, one inside a fence is
/// code, and so is one inside an inline span, which is where the extension
/// looks and does not find it.
#[test]
fn a_block_declares_the_identity_on_its_own_last_line() {
    let source = concat!(
        "[](){#empty-link-id}\n\n",
        "A paragraph.\n{#standalone-id}\n\n",
        "Trailing on the same line. {#not-an-identity}\n\n",
        "`{#inside-inline-code}`\n\n",
        "A paragraph whose last line is code.\n`{#code-on-the-last-line}`\n\n",
        "```text\n{#inside-a-fence}\n```\n"
    );
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(
        got.declared_anchors,
        vec!["empty-link-id".to_owned(), "standalone-id".to_owned()]
    );
}

/// Docusaurus writes the identity as an MDX comment, which the expression
/// grammar makes opaque, so the heading reads as if it carried none. The
/// identity is taken as written, case and all.
#[test]
fn an_mdx_comment_names_the_heading_it_ends() {
    names_the_heading(
        Adapter::Mdx,
        &[
            ("## `noIndex` {/* #noIndex */}\n", "noIndex", "noIndex"),
            (
                "## Spaced out {/*   #spaced-id   */}\n",
                "Spaced out",
                "spaced-id",
            ),
            ("## Nested braces {/* #a{b} */}\n", "Nested braces", "a{b}"),
        ],
    );
}

/// The forms the parser answers with no identity, each verdict taken from
/// `@docusaurus/utils` rather than from the syntax as it reads.
#[test]
fn an_mdx_comment_names_nothing_where_docusaurus_names_nothing() {
    for source in [
        "## Trailing text {/* #id */} after\n",
        "## Empty comment {/* */}\n",
        "## Two words {/* #two words */}\n",
    ] {
        let got = extraction(Adapter::Mdx, source);
        let heading = only(&got);
        assert!(
            heading.attribute.is_none(),
            "{source:?} declares no identity"
        );
    }
}

/// An item that is neither an identity, a class, nor a pair is not an
/// attribute block, which is what keeps a mangled MDX comment in a Markdown
/// file from naming a heading `attr_list` would not name.
#[test]
fn a_block_of_unknown_items_names_nothing() {
    for source in [
        "## Mangled comment {/ #mdx-id /}\n",
        "## Set {a, b}\n",
        "## Words {two words}\n",
    ] {
        let got = extraction(Adapter::Markdown, source);
        let heading = only(&got);
        assert!(heading.attribute.is_none(), "{source:?}");
        assert!(heading.text.contains('{'), "{source:?} keeps its braces");
    }
}

/// The block is read in the heading's own literal text, so one written as code
/// stays text and the heading keeps it.
#[test]
fn a_heading_block_written_as_code_names_nothing() {
    let got = extraction(Adapter::Markdown, "## Heading `{id=code-in-a-heading}`\n");
    let heading = only(&got);
    assert!(heading.attribute.is_none());
    assert_eq!(heading.text, "Heading {id=code-in-a-heading}");
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

/// github.com anchors a heading written as raw HTML, so its text is recorded
/// beside the Markdown ones: nested tags and comments contribute nothing,
/// references decode, and the whitespace a wrapped element carries survives.
#[test]
fn raw_html_headings_are_recorded_with_their_text_content() {
    let source = concat!(
        "<h1 align=\"center\"><code>tool</code></h1>\n\n",
        "<div>\n  <h2>A &amp; B<!-- note --> <em>c</em></h2>\n</div>\n\n",
        "<h3>\n  Wrapped\n</h3>\n\n",
        "<h4>&lt;b&gt; &quot;q&quot; &apos;a&apos; x&nbsp;y</h4>\n"
    );
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(
        texts(&got),
        vec![
            "tool".to_owned(),
            "A & B c".to_owned(),
            "\n  Wrapped\n".to_owned(),
            "<b> \"q\" 'a' x\u{a0}y".to_owned()
        ]
    );
    assert!(
        got.headings
            .iter()
            .all(|heading| heading.source == HeadingSource::RawHtml),
        "each came from raw HTML"
    );
}

/// The span covers the whole element, so a raw-HTML heading sorts into
/// document order with the Markdown headings around it.
#[test]
fn markdown_and_raw_html_headings_share_one_order() {
    let source = "# One\n\n<h2>Two</h2>\n\n### Three\n";
    let got = extraction(Adapter::Markdown, source);
    let rows: Vec<(String, HeadingSource, (usize, usize))> = got
        .headings
        .iter()
        .map(|heading| (heading.text.clone(), heading.source, heading.span))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("One".to_owned(), HeadingSource::Markdown, (0, 5)),
            ("Two".to_owned(), HeadingSource::RawHtml, (7, 19)),
            ("Three".to_owned(), HeadingSource::Markdown, (21, 30)),
        ]
    );
}

#[test]
fn an_unclosed_raw_html_heading_records_nothing() {
    let got = extraction(Adapter::Markdown, "<h2>Open\n\n<p>next</p>\n");
    assert!(got.headings.is_empty());
}

/// A region of openers with no closer costs one search per level, not one per
/// opener, so this returns instead of running for hours.
#[test]
fn a_region_of_unclosed_openers_stays_linear() {
    let source = format!("<div>{}</div>\n", "<h1 >".repeat(120_000));
    let got = extraction(Adapter::Markdown, &source);
    assert!(got.headings.is_empty());
}

/// An MDX element is opaque to the HTML scan, so `<h1>` in an MDX document is
/// a JSX element rather than a heading.
#[test]
fn a_raw_html_heading_in_mdx_records_nothing() {
    let got = extraction(Adapter::Mdx, "<h1>Title</h1>\n");
    assert!(got.headings.is_empty());
}

#[test]
fn a_document_without_headings_records_none() {
    let got = extraction(Adapter::Markdown, "text [a](b)\n");
    assert!(got.headings.is_empty());
    assert!(got.html_anchors.is_empty());
}
