#![cfg(test)]

use super::{ref_span, spelled_segments};

fn segments(tail: &str) -> Vec<String> {
    spelled_segments(tail).expect("the tail decodes")
}

/// Decoding runs once per segment, after the split: an escaped space
/// becomes the space the forge stores, a double-escaped percent stays one
/// escape deep, and an escaped slash lands inside its segment instead of
/// minting a new one. The trailing slash is the directory marker the plan
/// carried, not a segment.
#[test]
fn each_segment_decodes_exactly_once_in_place() {
    assert_eq!(segments("main/My%20File.md"), vec!["main", "My File.md"]);
    assert_eq!(
        segments("main/My%2520File.md"),
        vec!["main", "My%20File.md"]
    );
    assert_eq!(segments("release%2Fx/a.md"), vec!["release/x", "a.md"]);
    assert_eq!(segments("main/docs/"), vec!["main", "docs"]);
    assert_eq!(
        spelled_segments("main/%FF.md"),
        None,
        "escapes naming non-UTF-8 bytes fit no forge name: no spelling"
    );
}

/// A ref match must end where the URL ended a segment: `feature/x` spans
/// two written segments or one segment whose %2F the decode revealed, and
/// a candidate stopping mid-segment is no match at all.
#[test]
fn a_ref_ends_only_on_a_segment_boundary() {
    let written = segments("feature/x/docs/a.md");
    assert_eq!(ref_span(&written, "feature/x"), Some(2));
    assert_eq!(ref_span(&written, "feature"), Some(1));
    assert_eq!(ref_span(&written, "feature-x"), None);
    assert_eq!(ref_span(&written, "feature/xy"), None);
    assert_eq!(ref_span(&written, "feature/x/docs/a.md/deeper"), None);
    let escaped = segments("release%2Fx/a.md");
    assert_eq!(ref_span(&escaped, "release/x"), Some(1));
    assert_eq!(
        ref_span(&escaped, "release"),
        None,
        "the escaped slash belongs to the segment, not to the grammar"
    );
}
