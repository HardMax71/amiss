use amiss_wire::human::atom;

#[test]
fn printable_ascii_stays_literal_inside_quotes() {
    assert_eq!(atom("docs/guide.md"), "\"docs/guide.md\"");
    assert_eq!(atom(""), "\"\"");
    assert_eq!(atom("a b~!"), "\"a b~!\"");
}

#[test]
fn quote_and_backslash_escape_and_nothing_else_is_active() {
    assert_eq!(atom("a\"b"), "\"a\\\"b\"");
    assert_eq!(atom("a\\b"), "\"a\\\\b\"");
    assert_eq!(atom("a\nb\tc"), "\"a\\u000ab\\u0009c\"");
    assert_eq!(atom("\u{1b}[31m"), "\"\\u001b[31m\"", "ANSI is inert");
    assert_eq!(atom("\u{202e}"), "\"\\u202e\"", "bidi controls are inert");
}

#[test]
fn non_bmp_scalars_use_surrogate_pairs() {
    assert_eq!(atom("\u{1f600}"), "\"\\ud83d\\ude00\"");
    assert_eq!(atom("nav\u{e9}"), "\"nav\\u00e9\"");
}

#[test]
fn two_hundred_scalars_then_a_literal_ellipsis() {
    let long = "x".repeat(201);
    let rendered = atom(&long);
    assert_eq!(rendered.len(), 200 + 2 + 3);
    assert!(rendered.ends_with("...\""));
    let exact = "x".repeat(200);
    assert!(
        !atom(&exact).contains("..."),
        "exactly two hundred is not truncated"
    );

    let multibyte = "\u{e9}".repeat(201);
    let rendered = atom(&multibyte);
    assert!(rendered.ends_with("...\""));
    assert_eq!(
        rendered.matches("\\u00e9").count(),
        200,
        "the bound counts scalars"
    );
}
