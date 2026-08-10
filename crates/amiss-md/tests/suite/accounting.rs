use amiss_md::{AnalyzeError, Fault, Work, charge};
use amiss_wire::model::Adapter;
use amiss_wire::report::AnalysisErrorCode;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn markdown(source: &[u8]) -> Work {
    charge(Adapter::Markdown, source).expect("markdown charge")
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn plain(source: &[u8]) -> Work {
    charge(Adapter::PlainAdvisory, source).expect("plain charge")
}

#[test]
fn an_empty_document_charges_one_root() {
    assert_eq!(
        markdown(b""),
        Work {
            nodes: 1,
            nesting: 1
        }
    );
    assert_eq!(
        plain(b""),
        Work {
            nodes: 1,
            nesting: 1
        }
    );
}

#[test]
fn nodes_and_depth_follow_the_logical_tree() {
    assert_eq!(
        markdown(b"foo\n"),
        Work {
            nodes: 3,
            nesting: 3
        }
    );
    assert_eq!(
        markdown(b"> - [a](b)\n"),
        Work {
            nodes: 7,
            nesting: 7
        }
    );
}

#[test]
fn frontmatter_contributes_no_node() {
    let bare = markdown(b"# H\n");
    assert_eq!(markdown(b"---\na: b\n---\n# H\n"), bare);
    assert_eq!(markdown("\u{feff}---\na: b\n---\n# H\n".as_bytes()), bare);
}

#[test]
fn an_unclosed_opener_is_parsed_as_markdown() {
    let charged = markdown(b"---\na: b\n");
    assert_eq!(
        charged,
        Work {
            nodes: 4,
            nesting: 3
        }
    );
}

#[test]
fn plain_charges_one_paragraph_for_each_run() {
    assert_eq!(
        plain(b"a\n"),
        Work {
            nodes: 2,
            nesting: 2
        }
    );
    assert_eq!(
        plain(b"a\nb\n"),
        Work {
            nodes: 2,
            nesting: 2
        }
    );
    assert_eq!(
        plain(b"a\n\n\nb\n"),
        Work {
            nodes: 3,
            nesting: 2
        }
    );
    assert_eq!(
        plain(b"a\r\n\r\nb"),
        Work {
            nodes: 3,
            nesting: 2
        }
    );
    assert_eq!(
        plain(b"  \n\t\n \t \n"),
        Work {
            nodes: 1,
            nesting: 1
        }
    );
}

#[test]
fn only_a_parsing_adapter_requires_utf8() {
    let invalid = [0xffu8, 0xfe, 0x00];
    assert_eq!(
        charge(Adapter::Markdown, &invalid),
        Err(AnalyzeError::Fault(Fault::DocumentInvalid))
    );
    assert_eq!(
        charge(Adapter::PlainAdvisory, &invalid),
        Ok(Work {
            nodes: 2,
            nesting: 2
        })
    );
    assert_eq!(
        AnalysisErrorCode::from(Fault::DocumentInvalid).as_str(),
        "DOCUMENT_INVALID"
    );
    assert_eq!(
        AnalysisErrorCode::from(Fault::ParserPanic).as_str(),
        "PARSER_PANIC"
    );
}
