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

/// A parent directory that resolves outside the worktree refuses the write,
/// and one that resolves inside it does not.
#[cfg(unix)]
#[test]
fn a_symlinked_parent_escaping_the_worktree_refuses() {
    let outside = tempfile::TempDir::new().unwrap();
    let root = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("guide.md"), b"x").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("sub")).unwrap();
    let fixes = [Fix {
        start: 0,
        end: 1,
        replacement: "y".to_owned(),
    }];
    let staged = b"x".to_vec();
    let outcome = super::repair_document(root.path(), "sub/guide.md", &fixes, Some(&staged));
    assert!(
        matches!(
            outcome,
            super::DocumentOutcome::Refused("resolves outside the worktree")
        ),
        "the escape is named"
    );

    std::fs::create_dir(root.path().join("actual")).unwrap();
    std::fs::write(root.path().join("actual/guide.md"), b"x").unwrap();
    std::os::unix::fs::symlink(root.path().join("actual"), root.path().join("inside")).unwrap();
    let outcome = super::repair_document(root.path(), "inside/guide.md", &fixes, Some(&staged));
    assert!(
        matches!(outcome, super::DocumentOutcome::Applied(1)),
        "an in-root resolution applies"
    );
}
