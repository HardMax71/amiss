use amiss_wire::controls::{
    DOCUMENT_SUFFIX_BYTES, ProjectionKind, ProjectionSource, SOURCE_MARKER_BYTES, ScannerPolicy,
};
use amiss_wire::de::ErrorKind;

use crate::support::POLICY;

#[test]
fn parses_the_policy_fixture() {
    let policy = ScannerPolicy::parse(POLICY).unwrap();
    assert_eq!(policy.document_includes().len(), 2);
    assert_eq!(policy.projection_assertions().len(), 2);
    assert_eq!(policy.protected_inventory().len(), 2);
    assert_eq!(policy.finding_dispositions().len(), 1);
    assert_eq!(
        policy.document_includes()[0].suffix.as_deref(),
        Some(".txt")
    );
    let assertion = &policy.projection_assertions()[0];
    assert_eq!(assertion.document.as_str(), "docs/architecture.md");
    assert_eq!(assertion.name, "request-shape");
    assert_eq!(assertion.projection, ProjectionKind::CodeTextV1);
    let ProjectionSource::BlobLines(source) = &assertion.source else {
        panic!("fixture assertion uses blob lines");
    };
    assert_eq!(source.path.as_str(), "crates/amiss/src/request.rs");
    assert_eq!((source.first_line, source.last_line), (10, 14));
    let ProjectionSource::NamedRegion(source) = &policy.projection_assertions()[1].source else {
        panic!("fixture second assertion uses a named region");
    };
    assert_eq!(source.path.as_str(), "examples/generated.txt");
    assert_eq!(source.start_marker, "// amiss:generated:start");
    assert_eq!(source.end_marker, "// amiss:generated:end");
    assert_eq!(
        policy.digest(),
        ScannerPolicy::parse(POLICY).unwrap().digest()
    );
}

fn policy_with_assertions(assertions: &str) -> String {
    format!(
        r#"{{"schema":"amiss/scanner-policy","document_includes":[],"projection_assertions":[{assertions}],"protected_inventory":[],"finding_dispositions":[]}}"#
    )
}

#[test]
fn projection_assertions_have_one_closed_sorted_grammar() {
    let row = |document: &str, name: &str, first: u64, last: u64| {
        format!(
            r#"{{"document":"{document}","name":"{name}","projection":"code-text-v1","sink":"previous-code","source":{{"kind":"blob-lines","path":"src/lib.rs","first_line":{first},"last_line":{last}}}}}"#
        )
    };
    let valid = policy_with_assertions(&row("docs/a.md", "example", 1, 9_007_199_254_740_991));
    assert!(ScannerPolicy::parse(valid.as_bytes()).is_ok());

    let unsorted = policy_with_assertions(&format!(
        "{},{}",
        row("docs/b.md", "example", 1, 1),
        row("docs/a.md", "example", 1, 1)
    ));
    assert_eq!(
        ScannerPolicy::parse(unsorted.as_bytes()).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );

    let duplicate = policy_with_assertions(&format!(
        "{},{}",
        row("docs/a.md", "example", 1, 1),
        row("docs/a.md", "example", 2, 2)
    ));
    assert_eq!(
        ScannerPolicy::parse(duplicate.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember,
        "a selector change does not mint another assertion identity"
    );

    let reversed = policy_with_assertions(&row("docs/a.md", "example", 2, 1));
    assert_eq!(
        ScannerPolicy::parse(reversed.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let named = policy_with_assertions(
        r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"named-region","path":"src/lib.rs","start_marker":"// amiss:start","end_marker":"// amiss:end"}}"#,
    );
    let parsed = ScannerPolicy::parse(named.as_bytes()).unwrap();
    let ProjectionSource::NamedRegion(source) = &parsed.projection_assertions()[0].source else {
        panic!("named-region source survives the policy reader");
    };
    assert_eq!(source.path.as_str(), "src/lib.rs");
    assert_eq!(source.start_marker, "// amiss:start");
    assert_eq!(source.end_marker, "// amiss:end");
}

#[test]
fn projection_assertions_refuse_unknown_or_unsafe_words() {
    let valid = r#"{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{"kind":"blob-lines","path":"src/lib.rs","first_line":1,"last_line":1}}"#;
    for invalid in [
        valid.replace("\"name\":\"example\"", "\"name\":\"-example\""),
        valid.replace("code-text-v1", "code-text-v2"),
        valid.replace("previous-code", "next-code"),
        valid.replace("blob-lines", "blob-region"),
        valid.replace("\"first_line\":1", "\"first_line\":0"),
    ] {
        let policy = policy_with_assertions(&invalid);
        assert_eq!(
            ScannerPolicy::parse(policy.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "invalid row: {invalid}"
        );
    }
    let unsafe_integer =
        policy_with_assertions(&valid.replace("\"last_line\":1", "\"last_line\":9007199254740992"));
    assert!(matches!(
        ScannerPolicy::parse(unsafe_integer.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::Json(_)
    ));

    let named = |start: &str, end: &str| {
        policy_with_assertions(&format!(
            r#"{{"document":"docs/a.md","name":"example","projection":"code-text-v1","sink":"previous-code","source":{{"kind":"named-region","path":"src/lib.rs","start_marker":{start},"end_marker":{end}}}}}"#
        ))
    };
    for invalid in [
        named(r#"""#, r#""end""#),
        named(r#""   ""#, r#""end""#),
        named(r#""\tstart""#, r#""end""#),
        named(r#""same""#, r#""same""#),
        named(
            &format!("\"{}\"", "x".repeat(SOURCE_MARKER_BYTES.saturating_add(1))),
            r#""end""#,
        ),
    ] {
        assert!(
            ScannerPolicy::parse(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn rejects_policy_shape_defects() {
    let unknown = br#"{
      "schema": "amiss/scanner-policy",
      "document_includes": [],
      "protected_inventory": [],
      "finding_dispositions": [],
      "extra": 1
    }"#;
    assert_eq!(
        ScannerPolicy::parse(unknown).unwrap_err().kind,
        ErrorKind::UnknownField
    );

    let wrong_schema = br#"{
      "schema": "assure/scanner-policy",
      "document_includes": [],
      "protected_inventory": [],
      "finding_dispositions": []
    }"#;
    assert_eq!(
        ScannerPolicy::parse(wrong_schema).unwrap_err().kind,
        ErrorKind::InvalidValue
    );

    let unsorted = br#"{
      "schema": "amiss/scanner-policy",
      "document_includes": [],
      "protected_inventory": ["b.md", "a.md"],
      "finding_dispositions": []
    }"#;
    assert_eq!(
        ScannerPolicy::parse(unsorted).unwrap_err().kind,
        ErrorKind::UnsortedSet
    );

    for bad_path in ["/abs.md", "a//b.md", "a/../b.md", "a\\\\b.md", "a/./b.md"] {
        let doc = format!(
            r#"{{
              "schema": "amiss/scanner-policy",
              "document_includes": [],
              "protected_inventory": ["{bad_path}"],
              "finding_dispositions": []
            }}"#
        );
        assert_eq!(
            ScannerPolicy::parse(doc.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "path {bad_path}"
        );
    }
}

/// An include's optional adapter is a closed spelling: each wire id parses to
/// its adapter, absence stays unbound, and anything else refuses.
#[test]
fn an_include_binding_is_a_closed_adapter_spelling() {
    let bound = r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    let policy = ScannerPolicy::parse(bound.as_bytes()).unwrap();
    assert_eq!(
        policy.document_includes()[0].adapter,
        Some(amiss_wire::model::Adapter::Rst)
    );

    let unbound = r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    let policy = ScannerPolicy::parse(unbound.as_bytes()).unwrap();
    assert_eq!(policy.document_includes()[0].adapter, None);

    for bad in ["latex", "Rst", "restructuredtext", ""] {
        let doc = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"adapter":"{bad}","kind":"tree","path":"manual"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
        );
        assert_eq!(
            ScannerPolicy::parse(doc.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "adapter {bad}"
        );
    }
}

#[test]
fn a_tree_suffix_is_one_bounded_exact_selector() {
    let selected = r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"tree","path":"manual","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    let policy = ScannerPolicy::parse(selected.as_bytes()).unwrap();
    assert_eq!(
        policy.document_includes()[0].suffix.as_deref(),
        Some(".txt")
    );

    let longest = format!(".{}", "x".repeat(DOCUMENT_SUFFIX_BYTES.saturating_sub(1)));
    let at_limit = format!(
        r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"kind":"tree","path":"manual","suffix":"{longest}"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
    );
    assert!(ScannerPolicy::parse(at_limit.as_bytes()).is_ok());

    for suffix in ["", ".", "txt", ".a/b"] {
        let invalid = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"kind":"tree","path":"manual","suffix":"{suffix}"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
        );
        assert_eq!(
            ScannerPolicy::parse(invalid.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "suffix {suffix:?}"
        );
    }

    for invalid in [
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual","suffix":".a\\b"}],"protected_inventory":[],"finding_dispositions":[]}"#,
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual","suffix":".a\u0000b"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    ] {
        assert_eq!(
            ScannerPolicy::parse(invalid.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue
        );
    }

    let too_long = format!(".{}", "x".repeat(DOCUMENT_SUFFIX_BYTES));
    let multibyte_too_long = format!(".{}", "é".repeat(DOCUMENT_SUFFIX_BYTES / 2));
    for suffix in [too_long, multibyte_too_long] {
        let invalid = format!(
            r#"{{"schema":"amiss/scanner-policy","document_includes":[{{"kind":"tree","path":"manual","suffix":"{suffix}"}}],"protected_inventory":[],"finding_dispositions":[]}}"#
        );
        assert_eq!(
            ScannerPolicy::parse(invalid.as_bytes()).unwrap_err().kind,
            ErrorKind::InvalidValue,
            "the UTF-8 encoding crosses the byte ceiling"
        );
    }

    let document = r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"document","path":"manual.txt","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    assert_eq!(
        ScannerPolicy::parse(document.as_bytes()).unwrap_err().kind,
        ErrorKind::Inconsistent
    );

    let duplicate = r#"{"schema":"amiss/scanner-policy","document_includes":[{"kind":"tree","path":"manual","suffix":".rst"},{"kind":"tree","path":"manual","suffix":".txt"}],"protected_inventory":[],"finding_dispositions":[]}"#;
    assert_eq!(
        ScannerPolicy::parse(duplicate.as_bytes()).unwrap_err().kind,
        ErrorKind::DuplicateMember,
        "suffix does not mint a second selector identity at one root"
    );
}
