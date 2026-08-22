use amiss_wire::controls::{DOCUMENT_SUFFIX_BYTES, ScannerPolicy};
use amiss_wire::de::ErrorKind;

use crate::support::POLICY;

#[test]
fn parses_the_policy_fixture() {
    let policy = ScannerPolicy::parse(POLICY).unwrap();
    assert_eq!(policy.document_includes().len(), 2);
    assert_eq!(policy.protected_inventory().len(), 2);
    assert_eq!(policy.finding_dispositions().len(), 1);
    assert_eq!(
        policy.document_includes()[0].suffix.as_deref(),
        Some(".txt")
    );
    assert_eq!(
        policy.digest(),
        ScannerPolicy::parse(POLICY).unwrap().digest()
    );
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
