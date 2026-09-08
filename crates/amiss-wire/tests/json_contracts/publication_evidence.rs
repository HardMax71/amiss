use amiss_wire::{
    de::ErrorKind,
    publication::{self, PUBLICATION_DOCUMENT_BYTES, PUBLICATION_URI_BYTES, PublicationVerdict},
};

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
    assert_eq!(
        amiss_wire::write_json(&evidence, &mut oversized, PUBLICATION_DOCUMENT_BYTES)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    let exact = u64::try_from(oversized.len()).unwrap();
    assert!(exact > PUBLICATION_DOCUMENT_BYTES);
    assert_eq!(
        publication::parse_evidence(&oversized).unwrap_err().kind,
        ErrorKind::LimitExceeded
    );
    amiss_wire::write_json(&evidence, std::io::sink(), exact).unwrap();
    assert_eq!(
        amiss_wire::write_json(&evidence, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );

    let mut input = evidence.payload;
    input.deployment.record.uri.truncate(prefix.len());
    input.deployment.workflow.uri.truncate(prefix.len());
    let bounded = publication::evidence(input).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(&bounded, &mut bytes, PUBLICATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(publication::parse_evidence(&bytes).unwrap(), bounded);
}
