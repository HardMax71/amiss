use amiss_wire::{
    de::ErrorKind,
    publication::{
        self, PUBLICATION_DOCUMENT_BYTES, PUBLICATION_URI_BYTES, PublicationEvidenceEnvelope,
        PublicationVerdict,
    },
};

#[test]
fn publication_attempts_keep_the_safe_integer_contract_through_serde() {
    let example = include_str!("../../../../spec/examples/publication-evidence.json");
    let mut document = publication::parse_evidence(example.as_bytes()).unwrap();
    let maximum = amiss_wire::json::MAX_SAFE_INTEGER.unsigned_abs();
    for attempt in [1, maximum] {
        document.payload.deployment.provider_run_attempt = attempt;
        let envelope = publication::evidence(document.payload.clone()).unwrap();
        let encoded = serde_json::to_vec(&envelope).unwrap();
        assert_eq!(publication::parse_evidence(&encoded).unwrap(), envelope);
        assert_eq!(
            serde_json::from_slice::<PublicationEvidenceEnvelope>(&encoded).unwrap(),
            envelope
        );
        assert_eq!(
            envelope.payload_digest,
            amiss_wire::digest::hb(
                publication::EVIDENCE_PAYLOAD_SCHEMA,
                &serde_json_canonicalizer::to_vec(&envelope.payload).unwrap(),
            )
        );
    }
    for attempt in [maximum + 1, u64::MAX] {
        document.payload.deployment.provider_run_attempt = attempt;
        let invalid = example.replace(
            "\"provider_run_attempt\": 2",
            &format!("\"provider_run_attempt\": {attempt}"),
        );
        assert_ne!(invalid, example);
        assert_eq!(
            [
                serde_json::from_str::<PublicationEvidenceEnvelope>(&invalid).is_err(),
                serde_json::to_vec(&document).is_err(),
            ],
            [true; 2],
            "unsafe attempt {attempt} must fail at either Serde boundary"
        );
    }
    for attempt in ["-0", "2.0", "2e0"] {
        let invalid = example.replace(
            "\"provider_run_attempt\": 2",
            &format!("\"provider_run_attempt\": {attempt}"),
        );
        assert_ne!(invalid, example);
        assert!(serde_json::from_str::<PublicationEvidenceEnvelope>(&invalid).is_err());
        assert!(publication::parse_evidence(invalid.as_bytes()).is_err());
    }
}

#[test]
fn typed_evidence_moves_resources_but_output_still_enforces_the_aggregate_limit() {
    let plan = publication::parse_plan(include_bytes!(
        "../../../../spec/examples/publication-plan.json"
    ))
    .unwrap();
    let mut input = publication::parse_evidence(include_bytes!(
        "../../../../spec/examples/publication-evidence.json"
    ))
    .unwrap()
    .payload;
    let prefix = "https://example.com/";
    let allocations = [
        &mut input.deployment.record.uri,
        &mut input.deployment.workflow.uri,
        &mut input.target.canonical_url,
        &mut input.site.artifact.uri,
        &mut input.product.uri,
    ]
    .map(|uri| {
        *uri = format!(
            "{prefix}{}",
            "a".repeat(PUBLICATION_URI_BYTES - prefix.len())
        );
        uri.as_ptr()
    });
    let evidence = publication::evidence(input).unwrap();
    assert_eq!(
        allocations,
        [
            evidence.payload.deployment.record.uri.as_ptr(),
            evidence.payload.deployment.workflow.uri.as_ptr(),
            evidence.payload.target.canonical_url.as_ptr(),
            evidence.payload.site.artifact.uri.as_ptr(),
            evidence.payload.product.uri.as_ptr(),
        ]
    );
    let assessment = publication::assess(
        &plan,
        Some(&evidence),
        "1",
        amiss_wire::digest::sha256(b"engine"),
    )
    .unwrap();
    assert_eq!(assessment.payload.verdict, PublicationVerdict::Refuted);

    let mut oversized = Vec::new();
    assert!(matches!(
        amiss_wire::write_json(&evidence, &mut oversized, PUBLICATION_DOCUMENT_BYTES)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    ));
    let exact = u64::try_from(oversized.len()).unwrap();
    assert!(exact > PUBLICATION_DOCUMENT_BYTES);
    assert!(matches!(
        publication::parse_evidence(&oversized).unwrap_err().kind,
        ErrorKind::LimitExceeded
    ));
    amiss_wire::write_json(&evidence, std::io::sink(), exact).unwrap();
    assert!(matches!(
        amiss_wire::write_json(&evidence, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    ));

    let mut input = evidence.payload;
    input.deployment.record.uri.truncate(prefix.len());
    input.deployment.workflow.uri.truncate(prefix.len());
    let bounded = publication::evidence(input).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(&bounded, &mut bytes, PUBLICATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(publication::parse_evidence(&bytes).unwrap(), bounded);
}
