use std::fs;
use std::path::Path;

use amiss_scan::lfs::is_pointer;
use amiss_wire::json::{Value, parse};

/// The finite positive and negative corpus the spec pins for the conservative
/// LFS-pointer recognizer.
#[test]
fn the_pinned_vectors_decide_recognition() {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/lfs-pointer-vectors.json"),
    )
    .unwrap();
    let Value::Object(root) = parse(&bytes).unwrap() else {
        panic!("vectors are an object")
    };
    let field = |name: &str| {
        root.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    };
    assert_eq!(
        field("schema"),
        Some(&Value::String("amiss/lfs-pointer-vectors".to_owned()))
    );
    assert_eq!(
        field("contract"),
        Some(&Value::String("lfs-pointer-conservative".to_owned()))
    );
    let Some((_, Value::Array(cases))) = root.iter().find(|(key, _)| key == "cases") else {
        panic!("vectors hold cases")
    };
    assert!(!cases.is_empty());
    for case in cases {
        let Value::Object(members) = case else {
            panic!("a case is an object")
        };
        let get = |name: &str| {
            members
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value)
        };
        let Some(Value::String(id)) = get("id") else {
            panic!("a case has an id")
        };
        let Some(Value::String(input)) = get("input") else {
            panic!("{id} has an input")
        };
        let Some(Value::Bool(expected)) = get("recognized") else {
            panic!("{id} has a verdict")
        };
        assert_eq!(is_pointer(input.as_bytes()), *expected, "{id}");
    }
}

#[test]
fn the_size_bound_and_final_ending_are_hard_edges() {
    let base = "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 0\n";
    assert!(is_pointer(base.as_bytes()));
    assert!(
        !is_pointer(base.trim_end().as_bytes()),
        "missing final ending"
    );
    assert!(
        !is_pointer(
            base.replace('\n', "\r\n")
                .replace("size 0\r\n", "size 0\n")
                .as_bytes()
        ),
        "mixed endings"
    );
    assert!(
        is_pointer(base.replace('\n', "\r\n").as_bytes()),
        "all-CRLF transform"
    );

    let mut oversized = String::from("version https://git-lfs.github.com/spec/v1\n");
    oversized
        .push_str("oid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n");
    oversized.push_str("size 1\n");
    let padding = "x".repeat(
        1_024_usize
            .saturating_sub(oversized.len())
            .saturating_sub(3),
    );
    oversized.push('z');
    oversized.push(' ');
    oversized.push_str(&padding);
    oversized.push('\n');
    assert!(oversized.len() >= 1_024);
    assert!(
        !is_pointer(oversized.as_bytes()),
        "1,024 bytes or more is content"
    );
}
