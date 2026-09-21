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

/// Text between tags is prose, so an apostrophe there opens no string and
/// the element still closes. Inside a tag the same byte bounds an attribute
/// value, and an expression in either place is code again.
#[test]
fn jsx_text_is_prose_and_a_tag_bounds_its_attributes() {
    assert_eq!(
        completeness("x = (<>we've been working</>)"),
        Completeness::Complete,
        "an apostrophe in prose between tags is a letter like any other"
    );
    assert_eq!(
        completeness("x = <span title='it'>hi</span>"),
        Completeness::Complete,
        "a quote inside a tag bounds the attribute value it opened"
    );
    assert_eq!(
        completeness("x = <span title='"),
        Completeness::Incomplete,
        "an attribute value left open leaves the tag open"
    );
    assert_eq!(
        completeness("x = <a>{'}'}</a>"),
        Completeness::Complete,
        "a brace inside a string inside an expression closes nothing"
    );
    assert_eq!(
        completeness("x = <a>text"),
        Completeness::Incomplete,
        "an element nobody closed leaves the reading inside it"
    );
}

/// A `<` opens an element only where one can follow, so a comparison and a
/// type argument stay the code they are.
#[test]
fn a_comparison_is_not_an_element() {
    assert_eq!(
        completeness("a < b && c > d"),
        Completeness::Complete,
        "a space after the angle is no element, and nothing opened"
    );
    assert_eq!(
        completeness("a<b"),
        Completeness::Complete,
        "a name before the angle ends an expression, so no element opens"
    );
    assert_eq!(
        completeness("useState<string>(x)"),
        Completeness::Complete,
        "a type argument follows a name and opens nothing"
    );
    assert_eq!(
        completeness("const it = <>it's</>;"),
        Completeness::Complete,
        "an equals sign before the angle cannot end an expression"
    );
}

/// An expression between tags holds elements of its own, and closing them
/// returns the reading to the expression rather than to the text around it,
/// so the brace that closes the expression still closes it.
#[test]
fn an_element_inside_an_expression_does_not_end_the_text_around_it() {
    assert_eq!(
        completeness("x = <p>{flag && <a href={r}>Read more</a>}</p>"),
        Completeness::Complete,
        "the inner element closes inside the expression that opened it"
    );
    assert_eq!(
        completeness("x = <p>{flag && <a href={r}>Read more</a>}"),
        Completeness::Incomplete,
        "the outer element is still open once the expression closes"
    );
    assert_eq!(
        completeness("x = <p>{list.map(i => (<b>{i}</b>))}</p>"),
        Completeness::Complete,
        "elements nested two expressions deep each return to their own"
    );
}
