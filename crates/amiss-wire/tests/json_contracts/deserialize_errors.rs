use amiss_wire::{
    controls::{TrustedTimeStatement, parse_trusted_time},
    de::ErrorKind,
};
use serde_json::error::Category;

#[test]
fn typed_ingress_keeps_the_standard_library_utf8_error_for_the_complete_input() {
    let example = include_bytes!("../../../../spec/examples/scanner-trusted-time-statement.json");
    let inputs = [
        [example.as_slice(), &[0xff]].concat(),
        [example.as_slice(), &[0xf0, 0x9f]].concat(),
        ["{\"é\":\"".as_bytes(), &[0xed, 0xa0, 0x80], b"\"}"].concat(),
    ];
    for input in inputs {
        let native = std::str::from_utf8(&input).unwrap_err();
        let error = parse_trusted_time(&input).unwrap_err();
        assert_eq!(error.path, "$");
        assert_eq!(error.kind, ErrorKind::Utf8(native));
        assert_eq!(error.kind.to_string(), native.to_string());
        let source = std::error::Error::source(&error)
            .and_then(std::error::Error::source)
            .unwrap();
        assert_eq!(source.downcast_ref::<std::str::Utf8Error>(), Some(&native));
    }
}

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
