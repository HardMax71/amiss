use std::fs;
use std::path::Path;

use amiss_scan::lfs::is_pointer;
#[derive(serde::Deserialize)]
struct Vectors {
    schema: String,
    contract: String,
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
struct Case {
    id: String,
    input: String,
    recognized: bool,
}

/// The finite positive and negative corpus the spec pins for the conservative
/// LFS-pointer recognizer.
#[test]
fn the_pinned_vectors_decide_recognition() {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/lfs-pointer-vectors.json"),
    )
    .unwrap();
    let vectors: Vectors = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(vectors.schema, "amiss/lfs-pointer-vectors");
    assert_eq!(vectors.contract, "lfs-pointer-conservative");
    assert!(!vectors.cases.is_empty());
    for case in vectors.cases {
        assert_eq!(
            is_pointer(case.input.as_bytes()),
            case.recognized,
            "{}",
            case.id
        );
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

    let mut at_bound = String::from("version https://git-lfs.github.com/spec/v1\n");
    at_bound
        .push_str("oid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n");
    at_bound.push_str("size 1\n");
    let padding = "x".repeat(1_023_usize.saturating_sub(at_bound.len()).saturating_sub(3));
    at_bound.push_str("z ");
    at_bound.push_str(&padding);
    at_bound.push('\n');
    assert_eq!(at_bound.len(), 1_023);
    assert!(
        is_pointer(at_bound.as_bytes()),
        "exactly 1,023 bytes is still a pointer"
    );
}

/// The key grammar refuses an empty key on its own clause: a line opening
/// with the separator has a lawful byte alphabet and fails only emptiness.
#[test]
fn an_empty_key_is_refused_alone() {
    let pointer = "version https://git-lfs.github.com/spec/v1\n x\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 0\n";
    assert!(!is_pointer(pointer.as_bytes()));
}

/// Exactly one of each closing key: a pointer missing its size line is
/// ordinary content however well the oid line reads.
#[test]
fn a_missing_size_line_is_content() {
    let pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n";
    assert!(!is_pointer(pointer.as_bytes()));
}
