#![expect(
    clippy::panic,
    reason = "test fixture reader reports malformed published vectors"
)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use amiss_md::frontmatter::{MAX_BYTES, Region, recognize};
use amiss_wire::json::{Value, parse};

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

fn object<'a>(value: &'a Value, context: &str) -> &'a [(String, Value)] {
    let Value::Object(members) = value else {
        panic!("{context} is an object")
    };
    members
}

fn member<'a>(members: &'a [(String, Value)], name: &str, context: &str) -> &'a Value {
    members
        .iter()
        .find(|(key, _value)| key == name)
        .map_or_else(
            || panic!("{context} has a {name} member"),
            |(_key, value)| value,
        )
}

fn string<'a>(members: &'a [(String, Value)], name: &str, context: &str) -> &'a str {
    let Value::String(value) = member(members, name, context) else {
        panic!("{context}.{name} is a string")
    };
    value
}

fn boolean(members: &[(String, Value)], name: &str, context: &str) -> bool {
    let Value::Bool(value) = member(members, name, context) else {
        panic!("{context}.{name} is a boolean")
    };
    *value
}

fn nonnegative_integer(members: &[(String, Value)], name: &str, context: &str) -> usize {
    let Value::Integer(value) = member(members, name, context) else {
        panic!("{context}.{name} is an integer")
    };
    usize::try_from(*value).unwrap_or_else(|_error| panic!("{context}.{name} is nonnegative"))
}

fn optional_string<'a>(
    members: &'a [(String, Value)],
    name: &str,
    context: &str,
) -> Option<&'a str> {
    match member(members, name, context) {
        Value::Null => None,
        Value::String(value) => Some(value),
        Value::Bool(_) | Value::Integer(_) | Value::Array(_) | Value::Object(_) => {
            panic!("{context}.{name} is a string or null")
        }
    }
}

fn optional_integer(members: &[(String, Value)], name: &str, context: &str) -> Option<usize> {
    match member(members, name, context) {
        Value::Null => None,
        Value::Integer(value) => Some(
            usize::try_from(*value)
                .unwrap_or_else(|_error| panic!("{context}.{name} is nonnegative")),
        ),
        Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
            panic!("{context}.{name} is an integer or null")
        }
    }
}

fn assert_shape(members: &[(String, Value)], expected: &[&str], context: &str) {
    let actual: BTreeSet<&str> = members.iter().map(|(key, _value)| key.as_str()).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(actual, expected, "{context} has the closed member set");
}

fn newline(members: &[(String, Value)], context: &str) -> &'static [u8] {
    match members
        .iter()
        .find(|(key, _value)| key == "newline")
        .map(|(_key, value)| value)
    {
        None => b"\n",
        Some(Value::String(value)) if value == "crlf" => b"\r\n",
        Some(Value::String(value)) if value == "cr" => b"\r",
        Some(Value::String(value)) if value == "lf" => b"\n",
        Some(Value::String(value)) => panic!("{context}.newline has unknown value {value:?}"),
        Some(
            Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Array(_) | Value::Object(_),
        ) => panic!("{context}.newline is a string"),
    }
}

struct Vector<'a> {
    id: &'a str,
    bom_count: usize,
    opener: &'a str,
    closer: Option<&'a str>,
    payload_bytes: usize,
    closer_at_eof: bool,
    expected: bool,
    expected_bytes: Option<usize>,
    ending: &'static [u8],
}

impl<'a> Vector<'a> {
    fn read(case: &'a Value) -> Self {
        let members = object(case, "frontmatter vector case");
        let has_newline = members.iter().any(|(key, _value)| key == "newline");
        let required = [
            "id",
            "bom_count",
            "opener",
            "closer",
            "payload_bytes",
            "closer_at_eof",
            "expected",
            "expected_frontmatter_bytes",
        ];
        let with_newline = [
            "id",
            "bom_count",
            "opener",
            "closer",
            "payload_bytes",
            "closer_at_eof",
            "expected",
            "expected_frontmatter_bytes",
            "newline",
        ];
        assert_shape(
            members,
            if has_newline {
                &with_newline
            } else {
                &required
            },
            "frontmatter vector case",
        );

        let id = string(members, "id", "frontmatter vector case");
        let bom_count = nonnegative_integer(members, "bom_count", id);
        assert!(bom_count <= 2, "{id}.bom_count is at most two");
        let payload_bytes = nonnegative_integer(members, "payload_bytes", id);
        assert!(
            payload_bytes <= MAX_BYTES.saturating_add(1),
            "{id}.payload_bytes stays within the recognizer boundary corpus"
        );
        Self {
            id,
            bom_count,
            opener: string(members, "opener", id),
            closer: optional_string(members, "closer", id),
            payload_bytes,
            closer_at_eof: boolean(members, "closer_at_eof", id),
            expected: boolean(members, "expected", id),
            expected_bytes: optional_integer(members, "expected_frontmatter_bytes", id),
            ending: newline(members, id),
        }
    }

    fn source(&self) -> Vec<u8> {
        let mut source = Vec::new();
        for _ in 0..self.bom_count {
            source.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        source.extend_from_slice(self.opener.as_bytes());
        source.extend_from_slice(self.ending);
        source.resize(source.len().saturating_add(self.payload_bytes), b'a');
        source.extend_from_slice(self.ending);
        if let Some(closer) = self.closer {
            source.extend_from_slice(closer.as_bytes());
            if !self.closer_at_eof {
                source.extend_from_slice(self.ending);
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
    let vectors = parse(&bytes).expect("frontmatter vectors are strict JSON");
    let root = object(&vectors, "frontmatter vectors");
    assert_shape(
        root,
        &["schema", "contract", "cases"],
        "frontmatter vectors",
    );
    assert_eq!(
        string(root, "schema", "frontmatter vectors"),
        "amiss/frontmatter-vectors"
    );
    assert_eq!(
        string(root, "contract", "frontmatter vectors"),
        "frontmatter"
    );

    let Value::Array(cases) = member(root, "cases", "frontmatter vectors") else {
        panic!("frontmatter vectors.cases is an array")
    };
    assert!(!cases.is_empty(), "frontmatter vectors are nonempty");

    let mut ids = BTreeSet::new();
    for case in cases {
        let vector = Vector::read(case);
        assert!(
            !vector.id.trim().is_empty(),
            "frontmatter vector IDs are nonempty"
        );
        assert!(
            ids.insert(vector.id),
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
