#![cfg(test)]

use super::{Completeness, completeness};

/// Every opener the lexer knows keeps the region open on its own, and every
/// one is broken separately, since a chunk that trips two proves neither.
#[test]
fn each_opener_alone_leaves_the_chunk_incomplete() {
    for source in ["'", "\"", "`", "//", "/*", "(", "[", "{", "`${", "`${("] {
        assert_eq!(completeness(source), Completeness::Incomplete, "{source:?}");
    }
    for source in [
        "''", "\"\"", "``", "//\n", "/**/", "()", "[]", "{}", "`${}`", "`$`", "1 / 2",
    ] {
        assert_eq!(completeness(source), Completeness::Complete, "{source:?}");
    }
}

/// Inside a template the backslash takes the next byte with it, so an escaped
/// backtick does not close the template it stands in.
#[test]
fn a_template_escape_swallows_its_closer() {
    assert_eq!(completeness("`\\`"), Completeness::Incomplete);
    assert_eq!(completeness("`\\``"), Completeness::Complete);
}

/// A substitution opens on the exact pair and nothing else, and the code it
/// opens is code again.
#[test]
fn a_substitution_needs_the_pair_and_returns_to_code() {
    assert_eq!(
        completeness("`${`"),
        Completeness::Incomplete,
        "the substitution reopens code, and the backtick opens a template inside it"
    );
    assert_eq!(
        completeness("`$}`"),
        Completeness::Complete,
        "a dollar before anything else is an ordinary byte"
    );
    assert_eq!(
        completeness("`${'}'}`"),
        Completeness::Complete,
        "a brace inside a string in a substitution closes nothing"
    );
}

/// A block comment ends on the exact pair, so a star inside it is just a star.
#[test]
fn a_block_comment_ends_only_on_its_pair() {
    assert_eq!(completeness("/* * "), Completeness::Incomplete);
    assert_eq!(completeness("/* / "), Completeness::Incomplete);
    assert_eq!(completeness("/* * */"), Completeness::Complete);
}
