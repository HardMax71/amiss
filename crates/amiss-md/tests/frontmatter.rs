use amiss_md::frontmatter::{MAX_BYTES, Region, recognize};

#[test]
fn recognizes_a_yaml_region() {
    let source = b"---\ntitle: x\n---\nbody\n";
    assert_eq!(
        recognize(source),
        Some(Region {
            bom_bytes: 0,
            bytes: 17,
            suffix_offset: 17,
            suffix_line: 3,
        })
    );
}

#[test]
fn a_bom_precedes_the_region_without_joining_it() {
    let source = "\u{feff}---\na: b\n---\nx\n".as_bytes();
    let region = recognize(source).expect("region");
    assert_eq!(region.bom_bytes, 3);
    assert_eq!(region.bytes, 13);
    assert_eq!(region.suffix_offset, 16);
    assert_eq!(source.get(region.suffix_offset..), Some(b"x\n".as_slice()));
}

#[test]
fn dashes_also_close_with_dots_and_plus_closes_only_with_plus() {
    assert!(recognize(b"---\na: b\n...\nx\n").is_some());
    assert!(recognize(b"+++\na = 1\n+++\nx\n").is_some());
    assert!(recognize(b"+++\na = 1\n---\nx\n").is_none());
    assert!(recognize(b"+++\na = 1\n...\nx\n").is_none());
}

#[test]
fn a_closer_may_end_at_eof() {
    let region = recognize(b"---\na: b\n---").expect("region");
    assert_eq!(region.bytes, 12);
    assert_eq!(region.suffix_offset, 12);
}

#[test]
fn an_opener_without_a_closer_is_ordinary_markdown() {
    assert!(recognize(b"---\na: b\n").is_none());
    assert!(recognize(b"---").is_none());
    assert!(recognize(b"\n---\na: b\n---\n").is_none());
    assert!(recognize(b"--- \na: b\n---\n").is_none());
    assert!(recognize(b"text\n---\na: b\n---\n").is_none());
}

#[test]
fn crlf_and_bare_cr_are_single_endings() {
    let crlf = recognize(b"---\r\na: b\r\n---\r\nx").expect("crlf region");
    assert_eq!(crlf.bytes, 16);
    let cr = recognize(b"---\ra: b\r---\rx").expect("cr region");
    assert_eq!(cr.bytes, 13);
}

#[test]
fn the_region_ends_exactly_at_the_cap() {
    let filler = "a".repeat(65_527);
    let accepted = format!("---\n{filler}\n---\n");
    let region = recognize(accepted.as_bytes()).expect("region at the cap");
    assert_eq!(region.bytes, MAX_BYTES);

    let rejected = format!("---\n{filler}a\n---\n");
    assert!(
        recognize(rejected.as_bytes()).is_none(),
        "one byte past the cap is not a region"
    );
}

#[test]
fn the_first_permitted_closer_wins() {
    let region = recognize(b"---\na\n---\nb\n---\n").expect("region");
    assert_eq!(region.bytes, 10);
    assert_eq!(region.suffix_line, 3);
}
