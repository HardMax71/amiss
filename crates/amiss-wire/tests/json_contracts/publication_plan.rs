use amiss_wire::{
    de::ErrorKind,
    publication::{self, PUBLICATION_DOCUMENT_BYTES, PUBLICATION_URI_BYTES},
};

#[test]
fn typed_publication_plan_moves_its_resources_and_keeps_bounded_output() {
    let mut input = publication::parse_plan(include_bytes!(
        "../../../../spec/examples/publication-plan.json"
    ))
    .unwrap()
    .payload;
    let prefix = "https://example.com/";
    for uri in [
        &mut input.target.canonical_url,
        &mut input.site.artifact.uri,
        &mut input.product.uri,
    ] {
        *uri = format!(
            "{prefix}{}",
            "a".repeat(PUBLICATION_URI_BYTES - prefix.len())
        );
    }
    let allocations = [
        input.target.canonical_url.as_ptr(),
        input.site.artifact.uri.as_ptr(),
        input.product.uri.as_ptr(),
    ];
    let plan = publication::plan(input).unwrap();
    assert_eq!(
        allocations,
        [
            plan.payload.target.canonical_url.as_ptr(),
            plan.payload.site.artifact.uri.as_ptr(),
            plan.payload.product.uri.as_ptr()
        ]
    );
    let assessment =
        publication::assess(&plan, None, "1", amiss_wire::digest::sha256(b"engine")).unwrap();
    assert_eq!(
        assessment.payload.subject.plan_payload_digest,
        plan.payload_digest
    );

    let mut bytes = Vec::new();
    amiss_wire::write_json(&plan, &mut bytes, PUBLICATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(publication::parse_plan(&bytes).unwrap(), plan);
    let exact = u64::try_from(bytes.len()).unwrap();
    assert!(exact <= PUBLICATION_DOCUMENT_BYTES);
    amiss_wire::write_json(&plan, std::io::sink(), exact).unwrap();
    assert!(matches!(
        amiss_wire::write_json(&plan, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    ));
    let mut invalid = plan.payload;
    invalid.product.uri.push('a');
    let error = publication::plan(invalid).unwrap_err();
    assert_eq!(error.path, "$.payload.product.uri");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));
}
