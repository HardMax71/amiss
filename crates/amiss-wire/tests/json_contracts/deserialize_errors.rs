use amiss_wire::{
    JsonInputError,
    controls::{TrustedTimeStatement, parse_trusted_time},
    de::ErrorKind,
    digest::CanonicalJsonError,
};
use serde_json::error::Category;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct ScalarFlatten {
    #[serde(flatten, skip_deserializing)]
    count: u8,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct DroppedField {
    count: u8,
    #[serde(skip_serializing)]
    label: String,
}

#[test]
fn canonical_serialization_failures_retain_the_native_error() {
    let native = serde_json_canonicalizer::to_vec(&ScalarFlatten { count: 0 }).unwrap_err();
    let input = amiss_wire::read_json::<ScalarFlatten>(b"{}", 2).unwrap_err();
    let wire = amiss_wire::de::deserialize_json::<ScalarFlatten>(b"{}", "amiss/test").unwrap_err();
    assert!(matches!(
        input,
        JsonInputError::Canonical(CanonicalJsonError::Json(_))
    ));
    assert!(matches!(
        wire.kind,
        ErrorKind::Canonical(CanonicalJsonError::Json(_))
    ));
    assert_eq!(wire.path, "$");
    let errors: [&dyn std::error::Error; 2] = [&input, &wire];
    for error in errors {
        let source = std::iter::successors(Some(error), |source| source.source())
            .find_map(|source| source.downcast_ref::<serde_json::Error>())
            .expect("the canonical serializer's native error survives the source chain");
        assert_eq!(source.classify(), native.classify());
        assert_eq!(source.line(), native.line());
        assert_eq!(source.column(), native.column());
        assert_eq!(source.to_string(), native.to_string());
    }
}

#[test]
fn changed_canonical_input_is_a_domain_error_not_a_serde_failure() {
    let bytes = br#"{"count":1,"label":"retained input"}"#;
    let document: DroppedField = serde_json::from_slice(bytes).unwrap();
    assert_eq!(document.count, 1);
    assert_eq!(document.label, "retained input");
    let input = amiss_wire::read_json::<DroppedField>(bytes, 1024).unwrap_err();
    let wire = amiss_wire::de::deserialize_json::<DroppedField>(bytes, "amiss/test").unwrap_err();
    assert!(matches!(
        input,
        JsonInputError::Canonical(CanonicalJsonError::InputChanged)
    ));
    assert!(matches!(
        wire.kind,
        ErrorKind::Canonical(CanonicalJsonError::InputChanged)
    ));
    assert_eq!(wire.path, "$");
    let errors: [&dyn std::error::Error; 2] = [&input, &wire];
    for error in errors {
        assert!(
            std::iter::successors(Some(error), |source| source.source())
                .all(|source| source.downcast_ref::<serde_json::Error>().is_none())
        );
        assert!(
            error
                .to_string()
                .contains("typed document changes the JSON input")
        );
    }
}

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
