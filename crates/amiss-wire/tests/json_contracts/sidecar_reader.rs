use amiss_wire::{locale, publication, relation};

#[test]
fn sidecar_readers_reject_malformed_complete_inputs() -> Result<(), Box<dyn std::error::Error>> {
    let readers: [fn(&[u8]) -> bool; 9] = [
        |bytes| locale::parse_plan(bytes).is_ok(),
        |bytes| locale::parse_evidence(bytes).is_ok(),
        |bytes| locale::parse_assessment(bytes).is_ok(),
        |bytes| publication::parse_plan(bytes).is_ok(),
        |bytes| publication::parse_evidence(bytes).is_ok(),
        |bytes| publication::parse_assessment(bytes).is_ok(),
        |bytes| relation::parse_plan(bytes).is_ok(),
        |bytes| relation::parse_evidence(bytes).is_ok(),
        |bytes| relation::parse_assessment(bytes).is_ok(),
    ];
    for ((bytes, schema, limit), read) in [
        (
            include_bytes!("../../../../spec/examples/locale-coverage-plan.json").as_slice(),
            locale::PLAN_ENVELOPE_SCHEMA,
            locale::LOCALE_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/locale-coverage-evidence.json").as_slice(),
            locale::EVIDENCE_ENVELOPE_SCHEMA,
            locale::EVIDENCE_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/locale-coverage-assessment.json").as_slice(),
            locale::ASSESSMENT_ENVELOPE_SCHEMA,
            locale::ASSESSMENT_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/publication-plan.json").as_slice(),
            publication::PLAN_ENVELOPE_SCHEMA,
            publication::PUBLICATION_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/publication-evidence.json").as_slice(),
            publication::EVIDENCE_ENVELOPE_SCHEMA,
            publication::PUBLICATION_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/publication-assessment.json").as_slice(),
            publication::ASSESSMENT_ENVELOPE_SCHEMA,
            publication::PUBLICATION_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/relation-plan.json").as_slice(),
            relation::PLAN_ENVELOPE_SCHEMA,
            relation::RELATION_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/relation-evidence.json").as_slice(),
            relation::EVIDENCE_ENVELOPE_SCHEMA,
            relation::RELATION_DOCUMENT_BYTES,
        ),
        (
            include_bytes!("../../../../spec/examples/relation-assessment.json").as_slice(),
            relation::ASSESSMENT_ENVELOPE_SCHEMA,
            relation::RELATION_DOCUMENT_BYTES,
        ),
    ]
    .into_iter()
    .zip(readers)
    {
        super::input::assert_closed_input(bytes, schema, Some(limit), read)?;
    }
    Ok(())
}
