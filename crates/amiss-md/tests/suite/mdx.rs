use amiss_md::profile::parse_options;
use amiss_md::{AnalyzeError, Fault, Work, analyze, charge};
use amiss_wire::model::Adapter;
use markdown::mdast::Node;
use markdown::to_mdast;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn tree(source: &str) -> Node {
    let (options, _meter) = parse_options(Adapter::Mdx, u64::MAX).expect("mdx parse options");
    to_mdast(source, &options).expect("mdx parse")
}

/// The half-open raw byte interval of the first node of the named kind.
#[expect(clippy::expect_used, clippy::panic, reason = "test fixture helper")]
fn opaque(source: &str, kind: &str) -> (usize, usize) {
    let mut pending = vec![tree(source)];
    while let Some(node) = pending.pop() {
        let found = if matches!(
            node,
            Node::MdxTextExpression(_) | Node::MdxFlowExpression(_)
        ) {
            "expression"
        } else if matches!(node, Node::MdxjsEsm(_)) {
            "esm"
        } else if matches!(
            node,
            Node::MdxJsxTextElement(_) | Node::MdxJsxFlowElement(_)
        ) {
            "jsx"
        } else {
            ""
        };
        if found == kind {
            let position = node.position().expect("node position");
            return (position.start.offset, position.end.offset);
        }
        if let Some(children) = node.children() {
            pending.extend(children.iter().cloned());
        }
    }
    panic!("no {kind} node in {source:?}")
}

/// A brace inside a string is not the end of the code, so the region it opens
/// must stay opaque through it. Cutting it short is what would let the tail of
/// an expression be read as Markdown.
#[test]
fn a_brace_inside_a_string_does_not_close_the_region() {
    let source = "a {'}'} b";
    assert_eq!(opaque(source, "expression"), (2, 7));
    assert_eq!(source.get(2..7), Some("{'}'}"));
}

#[test]
fn a_brace_inside_a_comment_does_not_close_the_region() {
    let source = "a {/* } */} b";
    assert_eq!(opaque(source, "expression"), (2, 11));
    assert_eq!(source.get(2..11), Some("{/* } */}"));
}

#[test]
fn a_brace_inside_a_template_does_not_close_the_region() {
    let source = "a {`${'}'}`} b";
    assert_eq!(opaque(source, "expression"), (2, 12));
    assert_eq!(source.get(2..12), Some("{`${'}'}`}"));
}

#[test]
fn nested_braces_close_only_at_the_outermost() {
    let source = "a {{b: {c: 1}}} d";
    assert_eq!(opaque(source, "expression"), (2, 15));
}

/// An import whose brackets are still open runs on through a blank line, which
/// is what the pinned bundle does with a JavaScript parser in hand.
#[test]
fn an_open_bracket_carries_esm_across_a_blank_line() {
    let source = "export {\n\n  a\n\n} from \"b\"\n\nc\n";
    assert_eq!(opaque(source, "esm"), (0, 25));
    assert_eq!(source.get(0..25), Some("export {\n\n  a\n\n} from \"b\""));
}

/// A complete statement ends at the blank line that follows it.
#[test]
fn a_closed_statement_ends_esm_at_the_blank_line() {
    let source = "import a from \"b\"\n\nc\n";
    assert_eq!(opaque(source, "esm"), (0, 17));
}

/// Markdown inside a JSX element is still Markdown, and the element's own span
/// is what the interval law makes opaque.
#[test]
fn jsx_spans_its_whole_element() {
    let source = "<x>[a](b)</x>";
    assert_eq!(opaque(source, "jsx"), (0, 13));
}

/// `markdown-rs` 1.0.0 fails an internal assertion when a JSX tag opens inside a
/// link label and closes outside it. The contract's answer to a parser that
/// panics is to catch it and report `PARSER_PANIC`, never to abort the run.
#[test]
fn a_panicking_parser_is_caught_and_reported() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_silenced| {}));
    let link = charge(Adapter::Mdx, b"a [open <b> close](c) </b> d.");
    let image = charge(Adapter::Mdx, b"a ![open <b> close](c) </b> d.");
    std::panic::set_hook(previous);

    assert_eq!(link, Err(AnalyzeError::Fault(Fault::ParserPanic)));
    assert_eq!(image, Err(AnalyzeError::Fault(Fault::ParserPanic)));
}

/// MDX removes indented code, raw HTML, and plain autolinks, so a document that
/// charges differently under the two profiles proves the construct sets differ.
#[test]
fn mdx_removes_the_constructs_it_must() {
    let indented = b"    code\n";
    assert_eq!(
        charge(Adapter::Markdown, indented),
        Ok(Work {
            nodes: 2,
            nesting: 2
        })
    );
    assert_eq!(
        charge(Adapter::Mdx, indented),
        Ok(Work {
            nodes: 3,
            nesting: 3
        })
    );
}

/// The quadratic case from the corpus notes: a region that never closes makes
/// the parser ask at every brace, and every ask rescans the whole accumulated
/// region. The meter charges each ask before the scan runs, so the total the
/// document can demand is the allowance plus at most one further ask, and the
/// parse ends as a spent allowance rather than as a verdict about the source.
#[test]
fn an_unterminated_region_cannot_scan_past_its_allowance() {
    let source = format!("{{\"{}", "}".repeat(65_536));
    let allowance = 100_000_u64;
    let result = analyze(Adapter::Mdx, source.as_bytes(), allowance);
    let one_ask_past = allowance.saturating_add(u64::try_from(source.len()).unwrap_or(u64::MAX));
    let tripped = matches!(
        &result,
        Err(AnalyzeError::EmbeddedCodeAllowance { spent })
            if *spent > allowance && *spent <= one_ask_past
    );
    assert!(
        tripped,
        "the meter ends the parse within one ask of the line: {result:?}"
    );
}

/// The meter is exact and observes only real asks: the same document spends
/// the same bytes, an allowance equal to that spend still parses because only
/// crossing it trips, and the markdown grammar, which runs no embedded code,
/// spends nothing even under a zero allowance.
#[test]
fn spent_embedded_code_bytes_are_deterministic_and_sufficient() {
    let source = b"a {'}'} b\n\nexport {\n\n  a\n\n} from \"c\"\n";
    let first = analyze(Adapter::Mdx, source, u64::MAX).expect("mdx parse");
    let second = analyze(Adapter::Mdx, source, u64::MAX).expect("mdx parse");
    assert_eq!(first.embedded_code_bytes, second.embedded_code_bytes);
    assert!(first.embedded_code_bytes > 0);
    let exact =
        analyze(Adapter::Mdx, source, first.embedded_code_bytes).expect("the exact spend parses");
    assert_eq!(exact.embedded_code_bytes, first.embedded_code_bytes);
    let markdown = analyze(Adapter::Markdown, source, 0).expect("markdown parse");
    assert_eq!(markdown.embedded_code_bytes, 0);
}

/// An unmatched tag is a grammar rejection attributable to the source, which
/// the contract calls `DOCUMENT_INVALID` rather than a parser failure.
#[test]
fn an_unmatched_tag_is_an_invalid_document() {
    assert_eq!(
        charge(Adapter::Mdx, b"a <b> c"),
        Err(AnalyzeError::Fault(Fault::DocumentInvalid))
    );
    assert_eq!(
        charge(Adapter::Mdx, b"a <b></b> c"),
        Ok(Work {
            nodes: 5,
            nesting: 3
        })
    );
}
