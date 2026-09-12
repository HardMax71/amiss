use amiss_wire::{
    controls::{
        ProjectionKind, ProjectionSource, ScannerPolicy, TreePathSelection,
        canonical_scanner_policy, parse_projection_source, parse_scanner_policy,
    },
    de::ErrorKind,
};

#[test]
fn policy_assertion_presence_is_owned_by_serde_and_preserved_by_the_writer() {
    let mut document: ScannerPolicy = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-policy.json"
    ))
    .unwrap();
    document.projection_assertions = None;
    let absent = serde_json_canonicalizer::to_vec(&document).unwrap();
    let direct: ScannerPolicy = serde_json::from_slice(&absent).unwrap();
    assert_eq!(parse_scanner_policy(&absent).unwrap(), direct);
    assert_eq!(direct.projection_assertions, None);
    let (canonical, absent_digest) = canonical_scanner_policy(&direct).unwrap();
    assert_eq!(canonical, absent);

    document.projection_assertions = Some(Vec::new());
    let present = serde_json_canonicalizer::to_vec(&document).unwrap();
    let direct: ScannerPolicy = serde_json::from_slice(&present).unwrap();
    assert_eq!(parse_scanner_policy(&present).unwrap(), direct);
    assert_eq!(direct.projection_assertions, Some(Vec::new()));
    let (canonical, present_digest) = canonical_scanner_policy(&direct).unwrap();
    assert_eq!(canonical, present);
    assert_ne!(absent_digest, present_digest);

    let encoded = String::from_utf8(present).unwrap();
    for invalid in ["null", "false", "42", r#""""#, "{}"] {
        let altered = encoded.replace(
            "\"projection_assertions\":[]",
            &format!("\"projection_assertions\":{invalid}"),
        );
        assert_ne!(altered, encoded);
        assert!(serde_json::from_str::<ScannerPolicy>(&altered).is_err());
        let defect = parse_scanner_policy(altered.as_bytes()).unwrap_err();
        assert_eq!(defect.path, "$.projection_assertions");
        assert!(matches!(defect.kind, ErrorKind::Deserialize(source) if source.is_data()));
    }
}

#[test]
fn projection_suffix_preserves_absence_without_accepting_null() {
    let mut policy: ScannerPolicy = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-policy.json"
    ))
    .unwrap();
    let mut digests = Vec::new();
    for suffix in [Some(".md".to_owned()), None] {
        let source = ProjectionSource::TreePaths(TreePathSelection {
            root: "docs".parse().unwrap(),
            suffix,
            maximum_depth: 3,
        });
        let source_bytes = serde_json_canonicalizer::to_vec(&source).unwrap();
        assert_eq!(
            parse_projection_source(&source_bytes, ProjectionKind::SortedRowsV1).unwrap(),
            source
        );
        let assertion = &mut policy.projection_assertions.as_mut().unwrap()[0];
        assertion.projection = ProjectionKind::SortedRowsV1;
        assertion.source = source;
        let (bytes, digest) = canonical_scanner_policy(&policy).unwrap();
        assert_eq!(parse_scanner_policy(&bytes).unwrap(), policy);
        digests.push(digest);
    }
    assert_ne!(digests[0], digests[1]);
    let source = &policy.projection_assertions.as_ref().unwrap()[0].source;
    let encoded_source = serde_json::to_string(source).unwrap();
    let invalid_source = encoded_source.replacen('{', "{\"suffix\":null,", 1);
    let encoded_policy = serde_json::to_string(&policy).unwrap();
    let invalid_policy = encoded_policy.replace(&encoded_source, &invalid_source);
    assert_ne!(invalid_policy, encoded_policy);
    assert_eq!(
        [
            serde_json::from_str::<ProjectionSource>(&invalid_source).is_err(),
            parse_projection_source(invalid_source.as_bytes(), ProjectionKind::SortedRowsV1)
                .is_err(),
            serde_json::from_str::<ScannerPolicy>(&invalid_policy).is_err(),
            parse_scanner_policy(invalid_policy.as_bytes()).is_err(),
        ],
        [true; 4]
    );
}
