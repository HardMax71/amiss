use amiss_adoc::{Delimiter, ReferenceKind, Refusal, blocks, extract};

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn kinds(source: &str) -> Vec<(ReferenceKind, String)> {
    extract(source.as_bytes())
        .expect("utf-8 source")
        .references
        .into_iter()
        .map(|reference| (reference.kind, reference.target))
        .collect()
}

#[test]
fn every_core_reference_form_is_read_with_its_exact_target() {
    let source = concat!(
        "= Title\n\n",
        "See xref:other.adoc#section[Other] and link:https://example.com[Site].\n\n",
        "Also <<anchor,text>> and <<bare>>.\n\n",
        "image::img/logo.png[Logo]\n\n",
        "Inline image:img/icon.png[] here.\n\n",
        "include::shared/intro.adoc[]\n",
    );
    assert_eq!(
        kinds(source),
        vec![
            (
                ReferenceKind::CrossReference,
                "other.adoc#section".to_owned()
            ),
            (ReferenceKind::Link, "https://example.com".to_owned()),
            (ReferenceKind::InternalCrossReference, "anchor".to_owned()),
            (ReferenceKind::InternalCrossReference, "bare".to_owned()),
            (ReferenceKind::BlockImage, "img/logo.png".to_owned()),
            (ReferenceKind::InlineImage, "img/icon.png".to_owned()),
            (ReferenceKind::Include, "shared/intro.adoc".to_owned()),
        ],
    );
}

#[test]
fn a_span_selects_the_whole_macro_and_nothing_around_it() {
    let source = "Read xref:guide.adoc[Guide] now.\n";
    let extraction = extract(source.as_bytes()).expect("utf-8 source");
    let reference = extraction.references.first().expect("one reference");
    assert_eq!(
        source.get(reference.span.0..reference.span.1),
        Some("xref:guide.adoc[Guide]"),
    );
}

#[test]
fn code_is_never_a_reference() {
    for source in [
        "----\nxref:guide.adoc[Guide]\n----\n",
        "....\nlink:guide.adoc[Guide]\n....\n",
        "A `xref:guide.adoc[Guide]` span.\n",
        "An +link:guide.adoc[Guide]+ passthrough.\n",
        "An escaped \\xref:guide.adoc[Guide].\n",
        "The word prefixxref:guide.adoc[Guide].\n",
        "A bare xref:guide.adoc without brackets.\n",
        "A spaced xref:not a target[x].\n",
    ] {
        assert!(kinds(source).is_empty(), "{source:?} produced a reference");
    }
}

#[test]
fn a_container_block_does_not_repeat_the_references_inside_it() {
    let source = "====\nSee xref:guide.adoc[Guide].\n====\n";
    assert_eq!(
        kinds(source),
        vec![(ReferenceKind::CrossReference, "guide.adoc".to_owned())],
    );
}

#[test]
fn what_the_parser_will_not_read_into_is_declared() {
    let extraction =
        extract(b"++++\n<div>raw</div>\n++++\n\n////\nhidden\n////\n").expect("utf-8 source");
    assert_eq!(
        extraction.opaque.len(),
        1,
        "only passthrough holds output this parser cannot read; a comment holds nothing",
    );
    assert!(extraction.references.is_empty());
    let openers: Vec<Option<Delimiter>> = blocks("----\ncode\n----\n")
        .into_iter()
        .map(|block| block.delimiter)
        .collect();
    assert_eq!(openers, vec![Some(Delimiter::Verbatim)]);
}

#[test]
fn a_target_awaiting_an_attribute_says_so() {
    let extraction = extract(b"include::{includedir}/shared.adoc[]\n").expect("utf-8 source");
    let reference = extraction.references.first().expect("one include");
    assert!(reference.attribute_substituted());
    assert_eq!(reference.target, "{includedir}/shared.adoc");
    let plain = extract(b"include::shared.adoc[]\n").expect("utf-8 source");
    assert!(
        !plain
            .references
            .first()
            .expect("one include")
            .attribute_substituted()
    );
}

#[test]
fn titles_and_declared_anchors_carry_their_own_identity() {
    let extraction = extract(b"= Top\n\n[[explicit]]\n== Second Level\n\n[#hashed]\n=== Third\n")
        .expect("utf-8 source");
    let levels: Vec<(usize, &str)> = extraction
        .titles
        .iter()
        .map(|title| (title.level, title.text.as_str()))
        .collect();
    assert_eq!(levels, vec![(1, "Top"), (2, "Second Level"), (3, "Third")]);
    assert_eq!(extraction.anchors, vec!["explicit", "hashed"]);
}

#[test]
fn bytes_that_are_not_text_are_refused_rather_than_guessed_at() {
    assert_eq!(extract(&[0xff, 0xfe]), Err(Refusal::NotUtf8));
}
