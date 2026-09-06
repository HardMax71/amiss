use amiss_wire::{
    controls::{ScannerPolicy, canonical_scanner_policy, parse_scanner_policy},
    de::ErrorKind,
    json,
};
use serde_json::{Value, json};

#[test]
fn policy_assertion_presence_is_owned_by_serde_and_preserved_by_the_writer() {
    let mut document: Value = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-policy.json"
    ))
    .unwrap();
    document
        .as_object_mut()
        .unwrap()
        .remove("projection_assertions");
    let absent = serde_json::to_vec(&document).unwrap();
    let direct: ScannerPolicy = serde_json::from_slice(&absent).unwrap();
    assert_eq!(parse_scanner_policy(&absent).unwrap(), direct);
    assert_eq!(direct.projection_assertions, None);
    let (canonical, absent_digest) = canonical_scanner_policy(&direct).unwrap();
    assert_eq!(canonical, json::canonical(&json::parse(&absent).unwrap()));

    document["projection_assertions"] = json!([]);
    let present = serde_json::to_vec(&document).unwrap();
    let direct: ScannerPolicy = serde_json::from_slice(&present).unwrap();
    assert_eq!(parse_scanner_policy(&present).unwrap(), direct);
    assert_eq!(direct.projection_assertions, Some(Vec::new()));
    let (canonical, present_digest) = canonical_scanner_policy(&direct).unwrap();
    assert_eq!(canonical, json::canonical(&json::parse(&present).unwrap()));
    assert_ne!(absent_digest, present_digest);

    for invalid in [Value::Null, json!(false), json!(42), json!(""), json!({})] {
        document["projection_assertions"] = invalid;
        let bytes = serde_json::to_vec(&document).unwrap();
        assert!(serde_json::from_slice::<ScannerPolicy>(&bytes).is_err());
        let defect = parse_scanner_policy(&bytes).unwrap_err();
        assert_eq!(defect.path, "$.projection_assertions");
        assert_eq!(defect.kind, ErrorKind::WrongType);
    }
}
