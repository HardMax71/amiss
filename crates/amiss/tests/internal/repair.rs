#![cfg(test)]

use super::{Fix, splice};

/// Touching spans are lawful: the overlap refusal begins strictly past the
/// boundary, so an edit ending where the next begins still applies.
#[test]
fn touching_spans_apply_and_overlapping_spans_refuse() {
    let touching = [
        Fix {
            start: 0,
            end: 2,
            replacement: "ab".to_owned(),
        },
        Fix {
            start: 2,
            end: 4,
            replacement: "cd".to_owned(),
        },
    ];
    assert_eq!(splice(b"wxyz", &touching), Some(b"abcd".to_vec()));
    let overlapping = [
        Fix {
            start: 0,
            end: 3,
            replacement: "a".to_owned(),
        },
        Fix {
            start: 2,
            end: 4,
            replacement: "b".to_owned(),
        },
    ];
    assert_eq!(splice(b"wxyz", &overlapping), None);
}

/// Each out-of-range shape refuses alone: an inverted span, and an end past
/// the source, at the boundary and past it.
#[test]
fn out_of_range_spans_refuse_alone() {
    let inverted = [Fix {
        start: 3,
        end: 2,
        replacement: String::new(),
    }];
    assert_eq!(splice(b"wxyz", &inverted), None);
    let insertion = [Fix {
        start: 2,
        end: 2,
        replacement: "!".to_owned(),
    }];
    assert_eq!(splice(b"wxyz", &insertion), Some(b"wx!yz".to_vec()));
    let at_end = [Fix {
        start: 2,
        end: 4,
        replacement: "zz".to_owned(),
    }];
    assert_eq!(splice(b"wxyz", &at_end), Some(b"wxzz".to_vec()));
    let past_end = [Fix {
        start: 2,
        end: 5,
        replacement: String::new(),
    }];
    assert_eq!(splice(b"wxyz", &past_end), None);
}
