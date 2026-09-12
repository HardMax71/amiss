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
        assert!(matches!(error.kind, ErrorKind::Utf8(source) if source == native));
        assert_eq!(error.kind.to_string(), native.to_string());
        let source = std::error::Error::source(&error)
            .and_then(std::error::Error::source)
            .unwrap();
        assert_eq!(source.downcast_ref::<std::str::Utf8Error>(), Some(&native));
    }
}

#[test]
fn typed_ingress_retains_native_sources_categories_positions_and_paths() {
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
        ("{}".to_owned(), Category::Data, "$"),
        (r#"{"future":null}"#.to_owned(), Category::Data, "$"),
        (
            r#"{"repository":{}}"#.to_owned(),
            Category::Data,
            "$.repository",
        ),
        (
            r#"{"provider_run_attempt":"wrong"}"#.to_owned(),
            Category::Data,
            "$.provider_run_attempt",
        ),
        (
            r#"{"provider_run_attempt":1,"provider_run_attempt":2}"#.to_owned(),
            Category::Data,
            "$",
        ),
    ] {
        let native = serde_json::from_str::<TrustedTimeStatement>(&input).unwrap_err();
        assert_eq!(native.classify(), category, "{input}");
        let error = parse_trusted_time(input.as_bytes()).unwrap_err();
        assert_eq!(error.path, path, "{input}");
        let ErrorKind::Deserialize(source) = &error.kind else {
            panic!("expected the original Serde error: {error:?}");
        };
        assert_eq!(source.classify(), category, "{input}");
        assert_eq!(source.line(), native.line(), "{input}");
        assert_eq!(source.column(), native.column(), "{input}");
        assert_eq!(source.to_string(), native.to_string(), "{input}");
        let chained = std::error::Error::source(&error)
            .and_then(std::error::Error::source)
            .and_then(|source| source.downcast_ref::<serde_json::Error>())
            .expect("the standard source chain retains the native Serde error");
        assert!(std::ptr::eq(source, chained));
    }
}

#[test]
fn domain_errors_do_not_fabricate_serde_sources() {
    let error = amiss_wire::de::Error {
        path: "$.payload_digest".to_owned(),
        kind: ErrorKind::DigestMismatch,
    };
    assert!(
        std::error::Error::source(&error)
            .and_then(std::error::Error::source)
            .is_none()
    );
    assert_eq!(
        error.to_string(),
        "digest does not match at $.payload_digest"
    );
}
