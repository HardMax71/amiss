use amiss_md::{Analysis, BlockKind, Extraction, analyze};
use amiss_wire::controls::SourceConstruct;
use amiss_wire::model::Adapter;

use crate::fixtures::harvest;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn extraction(adapter: Adapter, source: &str) -> Extraction {
    analyze(adapter, source.as_bytes(), u64::MAX)
        .expect("analyze")
        .extraction
        .expect("a parsing adapter extracts")
}

fn spans_by(
    source: &str,
    field: fn(&amiss_wire::extraction::Occurrence) -> Option<(usize, usize)>,
) -> Vec<Option<(usize, usize)>> {
    extraction(Adapter::Markdown, source)
        .occurrences
        .iter()
        .map(field)
        .collect()
}

fn triples(extraction: &Extraction) -> Vec<(SourceConstruct, String, String)> {
    extraction
        .occurrences
        .iter()
        .map(|entry| {
            (
                entry.construct,
                entry.raw_destination.clone(),
                entry.semantic_destination.clone(),
            )
        })
        .collect()
}

/// The full golden for one occurrence: construct, both destination
/// representations, the construct's own span, the child-index path to the
/// syntax node, and the selected block owner.
#[test]
fn an_inline_link_carries_every_golden() {
    let source = "- see [a](<x y> \"t\")\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(got.occurrences.len(), 1);
    let entry = got.occurrences.first().cloned();
    let Some(entry) = entry else {
        return;
    };
    assert_eq!(entry.construct, SourceConstruct::InlineLink);
    assert_eq!(entry.raw_destination, "x y");
    assert_eq!(entry.semantic_destination, "x y");
    assert_eq!(entry.span, (6, 20));
    assert_eq!(source.get(6..20), Some("[a](<x y> \"t\")"));
    assert_eq!(entry.node_path, vec![0, 0, 0, 1]);
    assert_eq!(entry.block_kind, BlockKind::ListItem);
    assert_eq!(entry.block_span, (0, 20));
    assert_eq!(source.get(0..20), Some("- see [a](<x y> \"t\")"));
}

/// Reference forms take the destination token of the first winning
/// definition, never the consuming label, and never a later duplicate.
#[test]
fn definition_precedence_is_first_in_document_order() {
    let source = "See [a][x], [a][], [x].\n\n[x]: /first \"t\"\n[a]: <img.png>\n[x]: /second\n";
    let got = triples(&extraction(Adapter::Markdown, source));
    assert_eq!(
        got,
        vec![
            (
                SourceConstruct::FullReferenceLink,
                "/first".to_owned(),
                "/first".to_owned()
            ),
            (
                SourceConstruct::CollapsedReferenceLink,
                "img.png".to_owned(),
                "img.png".to_owned()
            ),
            (
                SourceConstruct::ShortcutReferenceLink,
                "/first".to_owned(),
                "/first".to_owned()
            ),
        ]
    );
}

/// All four autolink forms share one construct; the raw token drops angle
/// brackets, while the semantic destination is what the grammar constructs:
/// verbatim for URI and protocol forms, `mailto:` for email, `http://` for
/// `www.` forms.
#[test]
fn autolink_forms_share_a_construct_and_differ_in_tokens() {
    let source =
        "Go to <http://a.b>, <user@example.com>, www.example.com/x, and https://c.d/e?f=(g).\n";
    let got = triples(&extraction(Adapter::Markdown, source));
    let expected: Vec<(SourceConstruct, String, String)> = [
        ("http://a.b", "http://a.b"),
        ("user@example.com", "mailto:user@example.com"),
        ("www.example.com/x", "http://www.example.com/x"),
        ("https://c.d/e?f=(g)", "https://c.d/e?f=(g)"),
    ]
    .iter()
    .map(|(raw, semantic)| {
        (
            SourceConstruct::Autolink,
            (*raw).to_owned(),
            (*semantic).to_owned(),
        )
    })
    .collect();
    assert_eq!(got, expected);
}

/// The two destination representations answer different questions: the raw
/// token preserves the spelling, the semantic destination decodes it. An
/// empty destination is empty in both.
#[test]
fn raw_and_semantic_destinations_diverge_on_entities() {
    let source = "[a](&amp;b) and [c]()\n";
    let got = triples(&extraction(Adapter::Markdown, source));
    assert_eq!(
        got,
        vec![
            (
                SourceConstruct::InlineLink,
                "&amp;b".to_owned(),
                "&b".to_owned()
            ),
            (SourceConstruct::InlineLink, String::new(), String::new()),
        ]
    );
}

/// An image label, unlike a link label, may hold links and code spans with
/// brackets; the label scanner must step over all of them to find the
/// destination. A link wrapping an image yields both occurrences in document
/// order, and constructs inside a flattened image label yield none.
#[test]
fn image_labels_nest_and_wrap() {
    let source = "![alt [x] `]` text](/img \"t\") and [![i](/a)](/b)\n";
    let got = triples(&extraction(Adapter::Markdown, source));
    assert_eq!(
        got,
        vec![
            (
                SourceConstruct::InlineImage,
                "/img".to_owned(),
                "/img".to_owned()
            ),
            (
                SourceConstruct::InlineLink,
                "/b".to_owned(),
                "/b".to_owned()
            ),
            (
                SourceConstruct::InlineImage,
                "/a".to_owned(),
                "/a".to_owned()
            ),
        ]
    );
}

/// The owner override order: a link in a heading has no owner in the list, so
/// the document root owns it; a table cell owns its links; a block quote's
/// paragraph owns its own.
#[test]
fn owners_follow_the_override_order() {
    let heading = extraction(Adapter::Markdown, "# [a](b)\n");
    let in_heading = heading.occurrences.first();
    assert_eq!(
        in_heading.map(|entry| entry.block_kind),
        Some(BlockKind::DocumentRoot)
    );
    assert_eq!(
        in_heading.map(|entry| entry.block_span),
        Some((0, "# [a](b)\n".len()))
    );

    let table = extraction(Adapter::Markdown, "| h |\n| - |\n| [a](b) |\n");
    assert_eq!(
        table.occurrences.first().map(|entry| entry.block_kind),
        Some(BlockKind::TableCell)
    );

    let quoted = extraction(Adapter::Markdown, "> [a](b)\n");
    assert_eq!(
        quoted.occurrences.first().map(|entry| entry.block_kind),
        Some(BlockKind::Paragraph)
    );

    let footnote = extraction(Adapter::Markdown, "Call[^1].\n\n[^1]: see [a](b)\n");
    assert_eq!(
        footnote.occurrences.first().map(|entry| entry.block_kind),
        Some(BlockKind::Paragraph)
    );
}

/// A destination may sit on the next line, and inside a block quote that line
/// resumes with the container's marker, which is line prefix rather than
/// destination bytes.
#[test]
fn multiline_definitions_cross_container_markers() {
    let indented = extraction(
        Adapter::Markdown,
        "   [foo]: \n      /url  \n           'the title'  \n\n[foo]\n",
    );
    assert_eq!(
        triples(&indented),
        vec![(
            SourceConstruct::ShortcutReferenceLink,
            "/url".to_owned(),
            "/url".to_owned()
        )]
    );

    let quoted = extraction(Adapter::Markdown, "> [foo]:\n> /url\n\n[foo]\n");
    assert_eq!(
        triples(&quoted),
        vec![(
            SourceConstruct::ShortcutReferenceLink,
            "/url".to_owned(),
            "/url".to_owned()
        )]
    );
}

/// Frontmatter shifts every published byte offset and no node path, and its
/// own bytes are the first opaque region.
#[test]
fn frontmatter_translates_spans_not_paths() {
    let source = "---\nt: \"[not](x)\"\n---\n[a](b)\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(got.opaque.frontmatter_bytes, 22);
    assert_eq!(got.occurrences.len(), 1);
    let entry = got.occurrences.first();
    assert_eq!(entry.map(|occurrence| occurrence.span), Some((22, 28)));
    assert_eq!(
        entry.map(|occurrence| occurrence.block_span),
        Some((22, 28))
    );
    assert_eq!(source.get(22..28), Some("[a](b)"));
    assert_eq!(
        entry.map(|occurrence| occurrence.node_path.clone()),
        Some(vec![0, 0])
    );
}

/// A governed definition behind frontmatter publishes its translated span,
/// so the slice it names in the raw document is the definition itself.
#[test]
fn frontmatter_translates_governed_spans_too() {
    let source = "---\nt: x\n---\n\n[amiss:a]: ./t.md \"v\"\n";
    let got = extraction(Adapter::Markdown, source);
    let spans: Vec<&str> = got
        .governed
        .iter()
        .filter_map(|definition| source.get(definition.span.0..definition.span.1))
        .collect();
    assert_eq!(spans, vec!["[amiss:a]: ./t.md \"v\""]);
}

/// A JSX element's outer span is opaque, so nothing inside it is extracted;
/// constructs outside it still are. ESM and expressions contribute their own
/// intervals.
#[test]
fn mdx_regions_swallow_their_children() {
    let source = "export const a = 1\n\n<X>[hidden](h)</X>\n\nsee [shown](s) {x + 1}\n";
    let got = extraction(Adapter::Mdx, source);
    assert_eq!(
        triples(&got),
        vec![(SourceConstruct::InlineLink, "s".to_owned(), "s".to_owned())]
    );
    assert_eq!(got.opaque.mdx.len(), 3);
    assert_eq!(got.opaque.mdx.first(), Some(&(0, 18)));
    assert_eq!(got.opaque.html, Vec::new());
}

/// The two hostile-MDX rows of the attack matrix, in one document. It would
/// write a file, spin forever, and open a socket, if anything ever evaluated it.
/// Nothing does, and nothing can: there is no JavaScript in this process. The
/// test is here anyway, because that is a claim about the future as much as the
/// present, and the day someone reaches for a JS-backed parser to improve MDX
/// fidelity, this is what has to stop them. The infinite loop is not decoration:
/// a parser that evaluated the expression would never return, so the test
/// finishing at all is the bounded-parse half of the proof.
#[test]
fn an_mdx_document_that_would_attack_if_evaluated_is_only_ever_read() {
    let sentinel = std::env::temp_dir().join("amiss-mdx-evaluation-sentinel");
    let _absent = std::fs::remove_file(&sentinel);

    let mut source = String::from(
        "import {writeFileSync} from \"node:fs\";\nexport const boom = writeFileSync(\"",
    );
    source.push_str(&sentinel.display().to_string());
    source.push_str(
        "\", \"the parser evaluated me\");\n\
         \n\
         {(() => { while (true) {} })()}\n\
         \n\
         <Evil src={fetch(\"http://127.0.0.1:1/exfiltrate\")}>[hidden](secret.md)</Evil>\n",
    );

    let got = extraction(Adapter::Mdx, &source);

    assert!(
        !sentinel.exists(),
        "the import ran and wrote {}",
        sentinel.display()
    );
    assert_eq!(
        triples(&got),
        Vec::new(),
        "nothing inside an opaque region is a reference, and that includes the URL \
         the JSX would have fetched and the link it wraps"
    );
    assert!(
        !got.opaque.mdx.is_empty(),
        "what it could not see into, it says it could not see into"
    );
}

/// Exactly adjacent regions union into one maximal interval.
#[test]
fn adjacent_mdx_regions_union() {
    let source = "x <a/><b/> y\n";
    let got = extraction(Adapter::Mdx, source);
    assert_eq!(got.opaque.mdx, vec![(2, 10)]);
}

/// Raw HTML intervals are the Html nodes' own spans: two inline tags separated
/// by text stay two regions, and a flow block swallowing a link-looking line
/// is one region with no occurrence.
#[test]
fn html_regions_are_node_spans() {
    let source = "a <b>c</b> d\n\n<div>\n[inside](x)\n</div>\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(got.occurrences, Vec::new());
    assert_eq!(got.opaque.html, vec![(2, 5), (6, 10), (14, 38)]);
    assert_eq!(got.opaque.mdx, Vec::new());
}

/// Spans under CRLF endings never split a pair and stay byte-exact.
#[test]
fn crlf_spans_are_byte_exact() {
    let source = "a [x](y)\r\nb [z](w)\r\n";
    let got = extraction(Adapter::Markdown, source);
    let spans: Vec<(usize, usize)> = got.occurrences.iter().map(|entry| entry.span).collect();
    assert_eq!(spans, vec![(2, 8), (12, 18)]);
}

/// Every golden in the corpus obeys the closed span contract: bounded,
/// ordered, non-splitting, with a disjoint opaque partition and the right
/// empty side per profile.
#[test]
fn corpus_extraction_invariants_hold() {
    let (cases, _skipped) = harvest();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_silenced| {}));
    let mut checked = 0_usize;
    for case in &cases {
        for adapter in [Adapter::Markdown, Adapter::Mdx] {
            let Ok(Analysis {
                extraction: Some(extraction),
                ..
            }) = analyze(adapter, case.source.as_bytes(), u64::MAX)
            else {
                continue;
            };
            checked = checked.saturating_add(1);
            let len = case.source.len();
            let raw = case.source.as_bytes();
            let no_split = |at: usize| {
                at == 0
                    || !(raw.get(at.wrapping_sub(1)) == Some(&b'\r') && raw.get(at) == Some(&b'\n'))
            };
            let mut previous_start = 0_usize;
            for entry in &extraction.occurrences {
                assert!(
                    entry.span.0 < entry.span.1 && entry.span.1 <= len,
                    "{}",
                    case.case_id()
                );
                assert!(entry.span.0 >= previous_start, "{}", case.case_id());
                previous_start = entry.span.0;
                assert!(entry.block_span.1 <= len, "{}", case.case_id());
                assert!(
                    no_split(entry.span.0) && no_split(entry.span.1),
                    "{}",
                    case.case_id()
                );
            }
            let mut regions: Vec<(usize, usize)> = Vec::new();
            regions.extend(extraction.opaque.mdx.iter().copied());
            regions.extend(extraction.opaque.html.iter().copied());
            regions.sort_unstable();
            let mut previous_end = 0_usize;
            for (index, region) in regions.iter().enumerate() {
                assert!(region.0 < region.1 && region.1 <= len, "{}", case.case_id());
                assert!(index == 0 || region.0 >= previous_end, "{}", case.case_id());
                previous_end = region.1;
            }
            match adapter {
                Adapter::Markdown => {
                    assert_eq!(extraction.opaque.mdx, Vec::new(), "{}", case.case_id());
                }
                Adapter::Mdx => {
                    assert_eq!(extraction.opaque.html, Vec::new(), "{}", case.case_id());
                }
                Adapter::AsciiDoc | Adapter::Rst | Adapter::PlainAdvisory => {}
            }
        }
    }
    std::panic::set_hook(previous);
    assert!(checked > 3000, "the sweep covered {checked} parses");
}

/// A definition is reserved exactly when its decoded label begins with
/// lowercase `amiss:`; the decode covers escapes and entities but never
/// case folding, and a reserved winner suppresses its consumers without
/// creating another occurrence.
#[test]
fn reserved_definitions_surface_and_suppress() {
    let source = "See [claim][amiss:claim-one] and [real][docs].\n\n\
                  [amiss:claim-one]: ./subject.md \"claim\"\n\
                  [docs]: ./real.md\n\
                  [amiss&colon;entity]: ./other.md\n\
                  [AMISS:upper]: ./case.md\n";
    let got = extraction(Adapter::Markdown, source);
    assert_eq!(
        triples(&got),
        vec![(
            SourceConstruct::FullReferenceLink,
            "./real.md".to_owned(),
            "./real.md".to_owned()
        )],
        "the reserved winner suppresses its consumer; the ordinary one stays"
    );
    assert_eq!(
        got.governed.len(),
        2,
        "escape decoding counts, case folding does not"
    );
    let spans: Vec<&str> = got
        .governed
        .iter()
        .filter_map(|definition| source.get(definition.span.0..definition.span.1))
        .collect();
    assert_eq!(
        spans,
        vec![
            "[amiss:claim-one]: ./subject.md \"claim\"",
            "[amiss&colon;entity]: ./other.md",
        ],
        "the span runs from the opening bracket through the title, excluding the ending"
    );
    let words: Vec<(&str, Option<&str>, &str, bool)> = got
        .governed
        .iter()
        .map(|definition| {
            (
                definition.label.as_str(),
                definition.title.as_deref(),
                definition.url.as_str(),
                definition.angled,
            )
        })
        .collect();
    assert_eq!(
        words,
        vec![
            ("amiss:claim-one", Some("claim"), "./subject.md", false),
            ("amiss:entity", None, "./other.md", false),
        ],
        "the governed words are the parser's decoded label, title, and url"
    );
}

/// The decoded words survive the syntax that hides them: an angled
/// destination sheds its brackets but keeps its flag, entities decode in the
/// title, and a missing title is absent rather than empty.
#[test]
fn a_governed_definition_carries_its_decoded_words() {
    let got = extraction(
        Adapter::Markdown,
        "[amiss:angled]: <amiss:value?path=a.md&line=L1> \"one\"\n\n\
         [amiss:bare]: amiss:future\n\n\
         [amiss:titled]: ./x.md \"a &amp; b\"\n",
    );
    let words: Vec<(&str, Option<&str>, &str, bool)> = got
        .governed
        .iter()
        .map(|definition| {
            (
                definition.label.as_str(),
                definition.title.as_deref(),
                definition.url.as_str(),
                definition.angled,
            )
        })
        .collect();
    assert_eq!(
        words,
        vec![
            (
                "amiss:angled",
                Some("one"),
                "amiss:value?path=a.md&line=L1",
                true
            ),
            ("amiss:bare", None, "amiss:future", false),
            ("amiss:titled", Some("a & b"), "./x.md", false),
        ]
    );
}

/// Duplicate precedence: a losing reserved duplicate still contributes its
/// governed occurrence but cannot suppress a consumer whose first winner is
/// ordinary, and a later ordinary duplicate cannot unsuppress a reserved
/// first winner.
#[test]
fn reserved_duplicates_follow_first_winner_precedence() {
    let ordinary_first = extraction(
        Adapter::Markdown,
        "[a][x]\n\n[x]: ./first.md\n[amiss:x]: ./never.md\n",
    );
    assert_eq!(
        ordinary_first.occurrences.len(),
        1,
        "the ordinary winner resolves"
    );
    assert_eq!(
        ordinary_first.governed.len(),
        1,
        "the loser still contributes"
    );

    let reserved_first = extraction(Adapter::Markdown, "[a][amiss:x]\n\n[amiss:x]: ./wins.md\n");
    assert_eq!(reserved_first.occurrences.len(), 0, "suppressed");
    assert_eq!(reserved_first.governed.len(), 1);
}

/// Escapes hide the byte that would otherwise end the token: a `\>` inside an
/// angle destination, a `\]` inside an image label.
#[test]
fn an_escape_hides_the_closing_byte_from_its_token() {
    let angled = extraction(Adapter::Markdown, "[a](<x\\>y>)\n");
    assert_eq!(
        triples(&angled)
            .first()
            .map(|(_, raw, _)| raw.clone())
            .unwrap_or_default(),
        "x\\>y",
        "the escaped angle stays inside the destination"
    );

    let bracketed = extraction(Adapter::Markdown, "![a\\]b](x.md)\n");
    assert_eq!(
        triples(&bracketed)
            .first()
            .map(|(_, raw, _)| raw.clone())
            .unwrap_or_default(),
        "x.md",
        "the escaped bracket does not end the image label"
    );
}

/// A code span inside an image label protects a bracket only up to a closing
/// run of exactly the opening length, and an unmatched run is literal even
/// when a matching run exists past the label.
#[test]
fn a_code_span_in_an_image_label_measures_its_runs() {
    let doubled = extraction(Adapter::Markdown, "![a ``]`` b](x.md)\n");
    assert_eq!(
        triples(&doubled)
            .first()
            .map(|(_, raw, _)| raw.clone())
            .unwrap_or_default(),
        "x.md",
        "a double-backtick span protects the bracket inside it"
    );

    let unmatched = extraction(Adapter::Markdown, "![`x](a.md)\n");
    assert_eq!(
        triples(&unmatched)
            .first()
            .map(|(_, raw, _)| raw.clone())
            .unwrap_or_default(),
        "a.md",
        "an unmatched backtick is literal and the label still closes"
    );
}

/// A definition inside a blockquote continues onto the next line through at
/// most three spaces of indent before the marker; four break the lazy chain.
#[test]
fn a_blockquote_definition_continues_through_three_spaces_at_most() {
    let continued = extraction(Adapter::Markdown, "[u][r]\n\n> [r]:\n  > /dest.md\n");
    assert_eq!(
        triples(&continued)
            .first()
            .map(|(_, raw, _)| raw.clone())
            .unwrap_or_default(),
        "/dest.md",
        "two spaces before the marker continue the definition"
    );

    let broken = extraction(Adapter::Markdown, "[u][r]\n\n> [r]:\n    > /dest.md\n");
    assert!(
        triples(&broken).is_empty(),
        "four spaces before the marker end the definition unresolved"
    );
}

/// Adjacent opaque constructs coalesce into one region, and block regions
/// are newline-separated, which is why the overlap check in span validation
/// can never see two touching regions.
#[test]
fn adjacent_opaque_constructs_coalesce() {
    let inline = extraction(Adapter::Markdown, "a <b></b><i></i> c\n");
    assert_eq!(
        inline.opaque.html.len(),
        1,
        "back-to-back inline html merges"
    );

    let mixed = extraction(Adapter::Mdx, "a {expr}<b>t</b> c\n");
    assert_eq!(
        mixed.opaque.mdx.len(),
        1,
        "an expression and a jsx element touching merge"
    );
}

/// A GFM task checkbox is a checkbox, not a shortcut reference, even when a
/// definition spells its label.
#[test]
fn a_task_checkbox_is_not_a_reference() {
    for adapter in [Adapter::Markdown, Adapter::Mdx] {
        let got = extraction(adapter, "- [x] done\n\n[x]: target.md\n");
        assert!(
            got.occurrences.is_empty(),
            "{adapter:?}: {:?}",
            got.occurrences
        );
    }
}

/// The fragment span names bytes only under certainty: a plain inline
/// destination yields the exact range, and percent spellings, entities,
/// reference definitions out of span, duplicate destination text, and
/// fragmentless links all yield nothing. Frontmatter still translates it.
#[test]
fn a_fragment_span_names_bytes_only_under_certainty() {
    let spans = |source: &str| spans_by(source, |occurrence| occurrence.fragment_span);
    assert_eq!(spans("[a](guide.md#setup)\n"), vec![Some((13, 18))]);
    let doc = "[a](guide.md#setup)\n";
    assert_eq!(&doc.as_bytes()[13..18], b"setup");
    assert_eq!(spans("[a](x.md#a%20b)\n"), vec![None]);
    assert_eq!(spans("[a](x.md#a&amp;b)\n"), vec![None]);
    assert_eq!(spans("[a](x.md)\n"), vec![None]);
    assert_eq!(spans("[x.md#a](x.md#a)\n"), vec![None]);
    assert_eq!(spans("[a][r]\n\n[r]: x.md#frag\n"), vec![None]);
    assert_eq!(
        spans("<a#b@example.com>\n"),
        vec![None],
        "an autolink's hash sits in an email local part, never a fragment"
    );
    let fronted = "---\nkey: value\n---\n[a](guide.md#setup)\n";
    let with_front = spans(fronted);
    let (start, end) = with_front[0].expect("the fragment survives frontmatter");
    assert_eq!(&fronted.as_bytes()[start..end], b"setup");
}

/// The path span rides the same certainty: a plain destination yields the
/// exact range, the part stops at the hash, an autolink is never a path,
/// and frontmatter still translates it.
#[test]
fn a_path_span_names_bytes_only_under_certainty() {
    let spans = |source: &str| spans_by(source, |occurrence| occurrence.path_span);
    let doc = "[a](Guide.md)\n";
    assert_eq!(spans(doc), vec![Some((4, 12))]);
    assert_eq!(&doc.as_bytes()[4..12], b"Guide.md");
    let with_fragment = "[a](guide.md#setup)\n";
    assert_eq!(spans(with_fragment), vec![Some((4, 12))]);
    assert_eq!(&with_fragment.as_bytes()[4..12], b"guide.md");
    assert_eq!(spans("[a](x%20.md)\n"), vec![None]);
    assert_eq!(
        spans("<user@example.com>\n"),
        vec![None],
        "an autolink is a URL or address, never a repository path"
    );
    let fronted = "---\nkey: value\n---\n[a](Guide.md)\n";
    let with_front = spans(fronted);
    let (start, end) = with_front[0].expect("the path survives frontmatter");
    assert_eq!(&fronted.as_bytes()[start..end], b"Guide.md");
}
