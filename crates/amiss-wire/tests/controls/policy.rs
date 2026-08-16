use amiss_wire::controls::ScannerPolicy;
use amiss_wire::de::ErrorKind;

use crate::support::POLICY;

#[test]
fn parses_the_policy_fixture() {
    let policy = ScannerPolicy::parse(POLICY).unwrap();
    assert_eq!(policy.document_includes().len(), 2);
    assert_eq!(policy.protected_inventory().len(), 2);
    assert_eq!(policy.finding_dispositions().len(), 1);
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
