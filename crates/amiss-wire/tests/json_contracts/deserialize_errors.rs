use amiss_wire::{
    controls::{TrustedTimeStatement, parse_trusted_time},
    de::ErrorKind,
};
use serde_json::error::Category;

#[test]
fn typed_ingress_retains_native_syntax_categories_positions_and_paths() {
    let example = include_str!("../../../../spec/examples/scanner-trusted-time-statement.json");
    assert!(parse_trusted_time(example.as_bytes()).is_ok());
    for (input, category, path) in [
        ("{".to_owned(), Category::Eof, "$.?"),
        (
            "{\n  \"schema\": !}".to_owned(),
            Category::Syntax,
            "$.schema",
        ),
        (
            "{\n  \"repository\": {\"host\":".to_owned(),
            Category::Eof,
            "$.repository.host",
        ),
        (format!("{example}{{}}"), Category::Syntax, "$"),
    ] {
        let native = serde_json::from_str::<TrustedTimeStatement>(&input).unwrap_err();
        assert_eq!(native.classify(), category, "{input}");
        let error = parse_trusted_time(input.as_bytes()).unwrap_err();
        assert_eq!(error.path, path, "{input}");
        assert_eq!(
            error.kind,
            ErrorKind::Deserialize {
                category,
                line: native.line(),
                column: native.column(),
            },
            "{input}"
        );
    }
}
