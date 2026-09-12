use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use amiss_md::frontmatter::{MAX_BYTES, Region, recognize};
use serde::Deserialize;

const REQUIRED_VECTOR_IDS: [&str; 11] = [
    "FM-001-no-bom-exact-bound",
    "FM-002-no-bom-over-bound",
    "FM-003-bom-exact-bound",
    "FM-004-bom-over-bound",
    "FM-005-plus-matched",
    "FM-006-mismatched-closer",
    "FM-007-no-closer",
    "FM-008-crlf-matched",
    "FM-009-bare-cr-matched-at-eof",
    "FM-010-whitespace-suffixed-closer",
    "FM-011-double-bom",
];
const DOCUMENT_SUFFIX: &[u8] = b"body";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vectors {
    schema: String,
    contract: String,
    cases: Vec<Vector>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Newline {
    #[default]
    Lf,
    CrLf,
    Cr,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    id: String,
    bom_count: usize,
    opener: String,
    #[serde(deserialize_with = "Option::deserialize")]
    closer: Option<String>,
    payload_bytes: usize,
    closer_at_eof: bool,
    expected: bool,
    #[serde(
        rename = "expected_frontmatter_bytes",
        deserialize_with = "Option::deserialize"
    )]
    expected_bytes: Option<usize>,
    #[serde(default)]
    newline: Newline,
}

impl Vector {
    fn source(&self) -> Vec<u8> {
        let ending: &[u8] = match self.newline {
            Newline::Lf => b"\n",
            Newline::CrLf => b"\r\n",
            Newline::Cr => b"\r",
        };
        let mut source = Vec::new();
        for _ in 0..self.bom_count {
            source.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        source.extend_from_slice(self.opener.as_bytes());
        source.extend_from_slice(ending);
        source.resize(source.len().saturating_add(self.payload_bytes), b'a');
        source.extend_from_slice(ending);
        if let Some(closer) = &self.closer {
            source.extend_from_slice(closer.as_bytes());
            if !self.closer_at_eof {
                source.extend_from_slice(ending);
                source.extend_from_slice(DOCUMENT_SUFFIX);
            }
        }
        source
    }

    fn check(&self) {
        let source = self.source();
        let actual = recognize(&source);
        assert_eq!(actual.is_some(), self.expected, "{} recognition", self.id);
        assert_eq!(
            actual.map(|region| region.bytes),
            self.expected_bytes,
            "{} frontmatter bytes",
            self.id
        );
        if let Some(region) = actual {
            assert_eq!(
                region.bom_bytes,
                self.bom_count.saturating_mul(3),
                "{} BOM bytes",
                self.id
            );
            assert_eq!(
                region.suffix_offset,
                region.bom_bytes.saturating_add(region.bytes),
                "{} suffix offset",
                self.id
            );
            if self.closer_at_eof {
                assert_eq!(
                    region.suffix_offset,
                    source.len(),
                    "{} reaches EOF",
                    self.id
                );
            } else {
                assert_eq!(
                    source.get(region.suffix_offset..),
                    Some(DOCUMENT_SUFFIX),
                    "{} resumes at the document suffix",
                    self.id
                );
            }
            assert_eq!(region.suffix_line, 3, "{} suffix line", self.id);
            assert!(
                region.bytes <= MAX_BYTES,
                "{} respects the byte bound",
                self.id
            );
        }
    }
}

#[test]
fn the_published_vectors_drive_the_production_recognizer() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/frontmatter-vectors.json");
    let bytes = fs::read(&path).expect("frontmatter vectors are readable");
    let vectors: Vectors =
        serde_json::from_slice(&bytes).expect("frontmatter vectors have the published shape");
    assert_eq!(vectors.schema, "amiss/frontmatter-vectors");
    assert_eq!(vectors.contract, "frontmatter");
    assert!(
        !vectors.cases.is_empty(),
        "frontmatter vectors are nonempty"
    );

    let mut ids = BTreeSet::new();
    for vector in &vectors.cases {
        assert!(
            vector.bom_count <= 2,
            "{}.bom_count is at most two",
            vector.id
        );
        assert!(
            vector.payload_bytes <= MAX_BYTES.saturating_add(1),
            "{}.payload_bytes stays within the recognizer boundary corpus",
            vector.id
        );
        assert!(
            !vector.id.trim().is_empty(),
            "frontmatter vector IDs are nonempty"
        );
        assert!(
            ids.insert(vector.id.as_str()),
            "frontmatter vector ID {:?} is unique",
            vector.id
        );
        vector.check();
    }
    for required in REQUIRED_VECTOR_IDS {
        assert!(
            ids.contains(required),
            "the published frontmatter corpus lost {required}"
        );
    }
}

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
