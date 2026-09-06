use amiss_wire::{
    requests::{ControlsRequest, SuppliedSemanticEvidence},
    semantic,
};

#[test]
fn sealed_semantic_evidence_decodes_as_an_envelope_not_an_arbitrary_object() {
    let document = semantic::parse(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();
    let request = ControlsRequest {
        semantic_evidence: vec![SuppliedSemanticEvidence {
            expected_context_digest: document.payload.producer.context_digest,
            value: document,
        }],
        ..ControlsRequest::default()
    };
    assert_eq!(
        ControlsRequest::parse(&request.canonical_bytes().unwrap()).unwrap(),
        request
    );
    let document = &request.semantic_evidence[0].value;
    let envelope = serde_json::to_string(document).unwrap();
    let encoded = serde_json::to_string(&request).unwrap();
    let positional =
        serde_json::to_string(&(document.schema, &document.payload, document.payload_digest))
            .unwrap();
    for invalid in [
        "null".to_owned(),
        "[]".to_owned(),
        "{}".to_owned(),
        "42".to_owned(),
        envelope.replacen('{', "{\"future\":true,", 1),
        positional,
    ] {
        let malformed = encoded.replace(&envelope, &invalid);
        assert_ne!(malformed, encoded);
        assert!(
            ControlsRequest::parse(malformed.as_bytes()).is_err(),
            "{invalid}"
        );
        assert!(
            serde_json::from_str::<ControlsRequest>(&malformed).is_err(),
            "{invalid}"
        );
    }
}
