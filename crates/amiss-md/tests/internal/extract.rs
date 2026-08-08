#![cfg(test)]

use amiss_wire::controls::SourceConstruct;
use amiss_wire::extraction::{BlockKind, Fault, Heading, HeadingSource, Occurrence, Opaque};

use super::{image_label_end, run_length, skip_code_span, skip_whitespace, validate};

const RAW: &[u8] = b"xx\r\nyyyy\rz";

fn occurrence(span: (usize, usize), block_span: (usize, usize)) -> Occurrence {
    Occurrence {
        construct: SourceConstruct::InlineLink,
        raw_destination: String::new(),
        semantic_destination: String::new(),
        span,
        node_path: Vec::new(),
        block_kind: BlockKind::Paragraph,
        block_span,
        fragment_span: None,
        path_span: None,
    }
}

fn heading(span: (usize, usize)) -> Heading {
    Heading {
        text: String::new(),
        attribute: None,
        source: HeadingSource::Markdown,
        span,
    }
}

fn checked(occurrences: &[Occurrence], headings: &[Heading], opaque: &Opaque) -> Result<(), Fault> {
    validate(occurrences, headings, opaque, 0, RAW.len(), RAW)
}

#[test]
fn the_valid_surface_holds_touching_regions_and_a_bare_carriage_return() {
    let opaque = Opaque {
        frontmatter_bytes: 0,
        mdx: vec![(4, 6)],
        html: vec![(6, 8)],
    };
    assert_eq!(
        checked(
            &[occurrence((0, 2), (0, 2)), occurrence((9, 10), (9, 10))],
            &[heading((4, 6))],
            &opaque,
        ),
        Ok(())
    );
}

#[test]
fn a_reversed_span_faults() {
    let bad = checked(&[occurrence((5, 4), (4, 6))], &[], &Opaque::default());
    assert_eq!(bad, Err(Fault::InvalidSourceSpan));
}

#[test]
fn a_span_past_the_suffix_faults() {
    let bad = checked(&[occurrence((0, 11), (0, 2))], &[], &Opaque::default());
    assert_eq!(bad, Err(Fault::InvalidSourceSpan));
}

#[test]
fn an_endpoint_splitting_a_crlf_pair_faults() {
    let starts = checked(&[occurrence((3, 5), (3, 5))], &[], &Opaque::default());
    assert_eq!(starts, Err(Fault::InvalidSourceSpan));
    let ends = checked(&[occurrence((1, 3), (1, 3))], &[], &Opaque::default());
    assert_eq!(ends, Err(Fault::InvalidSourceSpan));
}

#[test]
fn one_bad_leg_faults_an_occurrence() {
    let bad_span = checked(&[occurrence((5, 4), (4, 6))], &[], &Opaque::default());
    assert_eq!(bad_span, Err(Fault::InvalidSourceSpan));
    let bad_block = checked(&[occurrence((4, 6), (5, 4))], &[], &Opaque::default());
    assert_eq!(bad_block, Err(Fault::InvalidSourceSpan));
    let empty = checked(&[occurrence((5, 5), (4, 6))], &[], &Opaque::default());
    assert_eq!(empty, Err(Fault::InvalidSourceSpan));
}

#[test]
fn a_heading_span_is_held_to_the_same_contract() {
    let bad = checked(&[], &[heading((5, 4))], &Opaque::default());
    assert_eq!(bad, Err(Fault::InvalidSourceSpan));
    let empty = checked(&[], &[heading((5, 5))], &Opaque::default());
    assert_eq!(empty, Err(Fault::InvalidSourceSpan));
}

#[test]
fn opaque_regions_are_bounded_nonempty_and_disjoint() {
    let bad = Opaque {
        frontmatter_bytes: 0,
        mdx: vec![(5, 4)],
        html: Vec::new(),
    };
    assert_eq!(checked(&[], &[], &bad), Err(Fault::InvalidSourceSpan));
    let overlapping = Opaque {
        frontmatter_bytes: 0,
        mdx: vec![(4, 7)],
        html: vec![(6, 9)],
    };
    assert_eq!(
        checked(&[], &[], &overlapping),
        Err(Fault::InvalidSourceSpan)
    );
}

#[test]
fn a_backtick_run_stops_counting_at_its_limit() {
    assert_eq!(run_length(b"``", 0, 1), 1);
}

#[test]
fn a_code_span_scan_stops_before_a_backtick_at_its_limit() {
    assert_eq!(skip_code_span(b"`ab``", 0, 3), 1);
}

#[test]
fn an_image_label_closing_at_the_span_end_is_outside_it() {
    assert_eq!(image_label_end(b"![a]x", (0, 5)), Ok(3));
    assert_eq!(image_label_end(b"![ab]x", (0, 4)), Err(Fault::ParserError));
}

#[test]
fn a_blockquote_continuation_takes_at_most_three_spaces() {
    assert_eq!(skip_whitespace(b"\n> d", 0), 3);
    assert_eq!(skip_whitespace(b"\n   >d", 0), 5);
    assert_eq!(skip_whitespace(b"\n    >d", 0), 5);
}
