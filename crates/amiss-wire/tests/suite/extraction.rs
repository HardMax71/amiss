use std::collections::BTreeSet;

use amiss_wire::extraction::{BlockKind, HeadingSource};

/// The two extraction name tables are pinned by distinctness: an emptied
/// projection fails non-emptiness and a constant one fails uniqueness.
#[test]
fn block_and_heading_names_are_nonempty_and_distinct() {
    let blocks = [
        BlockKind::Paragraph,
        BlockKind::ListItem,
        BlockKind::TableCell,
        BlockKind::DocumentRoot,
    ]
    .map(Into::<&'static str>::into);
    let sources = [
        HeadingSource::Markdown,
        HeadingSource::AsciiDoc,
        HeadingSource::Rst,
        HeadingSource::RawHtml,
    ]
    .map(Into::<&'static str>::into);
    for table in [&blocks, &sources] {
        assert!(table.iter().all(|name| !name.is_empty()));
        let unique: BTreeSet<&str> = table.iter().copied().collect();
        assert_eq!(unique.len(), table.len(), "{table:?}");
    }
}

/// The shared fragment-span core, held in its own package: the happy range
/// is byte-exact, and every refusal clause answers alone.
#[test]
fn the_fragment_core_names_bytes_only_under_certainty() {
    use amiss_wire::extraction::fragment_span;
    let source = b"see [a](x.md#frag) end";
    assert_eq!(fragment_span(source, (4, 18), "x.md#frag"), Some((13, 17)));
    assert_eq!(&source[13..17], b"frag");
    assert_eq!(fragment_span(source, (4, 18), "x.md"), None, "no hash");
    assert_eq!(
        fragment_span(b"see [a](x.md#) end", (4, 14), "x.md#"),
        None,
        "empty fragment"
    );
    assert_eq!(
        fragment_span(b"see [a](x.md#a#b) end", (4, 17), "x.md#a#b"),
        None,
        "a second hash refuses on the fragment clause"
    );
    assert_eq!(
        fragment_span(b"see [a](x.md#a%20b) end", (4, 19), "x.md#a%20b"),
        None,
        "a percent fragment refuses alone"
    );
    assert_eq!(
        fragment_span(b"see [a](x.md#a&b) end", (4, 17), "x.md#a&b"),
        None,
        "an ampersand fragment refuses alone"
    );
    assert_eq!(
        fragment_span(b"see [a](x.md#a\\b) end", (4, 17), "x.md#a\\b"),
        None,
        "a backslash fragment refuses alone"
    );
    assert_eq!(
        fragment_span(b"see [a](x%20.md#ab) end", (4, 19), "x%20.md#ab"),
        None,
        "a percent prefix refuses alone"
    );
    assert_eq!(
        fragment_span(b"[x.md#a](x.md#a)", (0, 16), "x.md#a"),
        None,
        "two verbatim hits are ambiguity"
    );
    assert_eq!(
        fragment_span(b"see [a](other) end", (4, 14), "x.md#frag"),
        None,
        "an absent needle names nothing"
    );
    assert_eq!(
        fragment_span(source, (4, 200), "x.md#frag"),
        None,
        "a span past the source names nothing"
    );
}

/// The path-span core beside its sibling: the path part is byte-exact with
/// and without a fragment, and every refusal answers alone.
#[test]
fn the_path_core_names_bytes_only_under_certainty() {
    use amiss_wire::extraction::path_span;
    let plain = b"see [a](Guide.md) end";
    assert_eq!(path_span(plain, (4, 17), "Guide.md"), Some((8, 16)));
    assert_eq!(&plain[8..16], b"Guide.md");
    let with_fragment = b"see [a](Guide.md#x) end";
    assert_eq!(
        path_span(with_fragment, (4, 19), "Guide.md#x"),
        Some((8, 16))
    );
    assert_eq!(
        path_span(b"see [a](#x) end", (4, 11), "#x"),
        None,
        "a pure fragment holds no path part"
    );
    assert_eq!(
        path_span(b"see [a](G%20.md) end", (4, 16), "G%20.md"),
        None,
        "a percent path refuses alone"
    );
    assert_eq!(
        path_span(b"see [a](a&b.md) end", (4, 15), "a&b.md"),
        None,
        "an ampersand path refuses alone"
    );
    assert_eq!(
        path_span(b"see [a](a\\b.md) end", (4, 15), "a\\b.md"),
        None,
        "a backslash path refuses alone"
    );
    assert_eq!(
        path_span(b"see [a]() end", (4, 9), ""),
        None,
        "an empty destination names nothing"
    );
    assert_eq!(
        path_span(b"[Guide.md](Guide.md)", (0, 20), "Guide.md"),
        None,
        "two verbatim hits are ambiguity"
    );
}

/// The carrier line grammar, one clause at a time: every accepted part is
/// pinned exactly, and each refusal case would parse if its clause dropped.
#[test]
fn the_governed_carrier_line_is_closed_clause_by_clause() {
    use amiss_wire::extraction::governed_carrier_line;

    assert_eq!(
        governed_carrier_line(r#"[amiss:pin]: <amiss:value?path=a&line=L1> "alpha""#),
        Some((
            "amiss:pin".to_owned(),
            "amiss:value?path=a&line=L1".to_owned(),
            "alpha".to_owned(),
        )),
    );
    assert_eq!(
        governed_carrier_line("[amiss:q.2_x-y]: <d> 'single'"),
        Some((
            "amiss:q.2_x-y".to_owned(),
            "d".to_owned(),
            "single".to_owned()
        )),
    );
    assert_eq!(
        governed_carrier_line(r#"[amiss:e]: <d> """#),
        Some(("amiss:e".to_owned(), "d".to_owned(), String::new())),
        "an empty title claims an empty line"
    );

    for (reason, line) in [
        ("unreserved label", r#"[other:p]: <d> "t""#),
        ("empty name", r#"[amiss:]: <d> "t""#),
        ("bracket in name", r#"[amiss:a[b]: <d> "t""#),
        ("closing bracket in name", r#"[amiss:a]b]: <d> "t""#),
        ("empty destination", r#"[amiss:p]: <> "t""#),
        ("angle inside destination", r#"[amiss:p]: <a<b> "t""#),
        (
            "closing angle inside destination",
            r#"[amiss:p]: <a>b> "t""#,
        ),
        ("bare destination", r#"[amiss:p]: d "t""#),
        ("missing title", "[amiss:p]: <d>"),
        ("quote inside double title", r#"[amiss:p]: <d> "a"b""#),
        ("quote inside single title", "[amiss:p]: <d> 'a'b'"),
        ("unterminated title", r#"[amiss:p]: <d> "t"#),
        ("trailing bytes", r#"[amiss:p]: <d> "t" extra"#),
    ] {
        assert_eq!(governed_carrier_line(line), None, "{reason}");
    }
}
