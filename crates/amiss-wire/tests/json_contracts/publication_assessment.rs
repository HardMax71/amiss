use amiss_wire::{
    de::ErrorKind,
    digest::{hb, sha256},
    publication::{
        self, EVIDENCE_PAYLOAD_SCHEMA, PUBLICATION_DOCUMENT_BYTES, PublicationReason,
        PublicationVerdict,
    },
    semantic::PRODUCER_VERSION_BYTES,
};

#[test]
fn typed_publication_assessment_keeps_bounded_output_and_validated_engine_identity() {
    let plan = publication::parse_plan(include_bytes!(
        "../../../../spec/examples/publication-plan.json"
    ))
    .unwrap();
    let mut evidence = publication::parse_evidence(include_bytes!(
        "../../../../spec/examples/publication-evidence.json"
    ))
    .unwrap();
    evidence.payload.docs.candidate_identity_digest = sha256(b"other docs");
    evidence.payload.target.canonical_url = "https://preview.example.com/widget/".to_owned();
    evidence.payload.site.input_digest = sha256(b"other site");
    evidence.payload.product.digest = sha256(b"other product");
    evidence.payload_digest = hb(
        EVIDENCE_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&evidence.payload).unwrap(),
    );
    let version = "a".repeat(PRODUCER_VERSION_BYTES);
    let engine = sha256(b"publication evaluator");
    let assessment = publication::assess(&plan, Some(&evidence), &version, engine).unwrap();
    assert_eq!(assessment.payload.engine.engine_version, version);
    assert_eq!(assessment.payload.verdict, PublicationVerdict::Refuted);
    assert_eq!(
        assessment.payload.reasons,
        vec![
            PublicationReason::DocsMismatch,
            PublicationReason::TargetMismatch,
            PublicationReason::SiteMismatch,
            PublicationReason::ProductMismatch,
        ]
    );
    let mut bytes = Vec::new();
    amiss_wire::write_json(&assessment, &mut bytes, PUBLICATION_DOCUMENT_BYTES).unwrap();
    let exact = u64::try_from(bytes.len()).unwrap();
    assert!(exact <= PUBLICATION_DOCUMENT_BYTES);
    assert_eq!(publication::parse_assessment(&bytes).unwrap(), assessment);
    amiss_wire::write_json(&assessment, std::io::sink(), exact).unwrap();
    assert_eq!(
        amiss_wire::write_json(&assessment, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    for invalid in [String::new(), format!("{version}a"), "1 bad".to_owned()] {
        let error = publication::assess(&plan, Some(&evidence), &invalid, engine).unwrap_err();
        assert_eq!(error.path, "$.payload.engine.engine_version");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }
}
