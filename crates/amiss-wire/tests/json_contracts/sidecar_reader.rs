use amiss_wire::{locale, publication};

#[test]
fn locale_and_publication_readers_reject_malformed_complete_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    let readers: [fn(&[u8]) -> bool; 6] = [
        |bytes| locale::parse_plan(bytes).is_ok(),
        |bytes| locale::parse_evidence(bytes).is_ok(),
        |bytes| locale::parse_assessment(bytes).is_ok(),
        |bytes| publication::parse_plan(bytes).is_ok(),
        |bytes| publication::parse_evidence(bytes).is_ok(),
        |bytes| publication::parse_assessment(bytes).is_ok(),
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
    ]
    .into_iter()
    .zip(readers)
    {
        super::input::assert_closed_input(bytes, schema, limit, read)?;
    }
    Ok(())
}
