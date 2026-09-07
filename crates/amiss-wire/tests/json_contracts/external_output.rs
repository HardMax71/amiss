use amiss_wire::{
    de::ErrorKind,
    external::{
        EXTERNAL_DOCUMENT_BYTES, ExternalEvidenceProducer, assess, parse_assessment, parse_plan,
        plan,
    },
    report::validate_envelope,
    write_json,
};

#[test]
fn external_outputs_own_their_data_and_replay_the_committed_artifacts() {
    let expected_plan = parse_plan(include_bytes!(
        "../../../../spec/examples/scanner-external-plan.json"
    ))
    .unwrap();
    let expected_assessment = parse_assessment(include_bytes!(
        "../../../../spec/examples/scanner-external-assessment.json"
    ))
    .unwrap();
    let (planned, assessed) = {
        let (report, _) = validate_envelope(include_bytes!(
            "../../../../spec/examples/scanner-report.json"
        ))
        .unwrap();
        let engine = &expected_plan.payload.engine;
        let planned = plan(&report, &engine.engine_version, engine.engine_digest).unwrap();
        let mut plan_bytes = Vec::new();
        write_json(&planned, &mut plan_bytes, EXTERNAL_DOCUMENT_BYTES).unwrap();
        assert_eq!(
            plan_bytes,
            serde_json_canonicalizer::to_vec(&expected_plan).unwrap()
        );
        assert_eq!(parse_plan(&plan_bytes).unwrap(), planned);
        let evidence = include_bytes!("../../../../spec/examples/scanner-external-evidence.json");
        let engine = &expected_assessment.payload.engine;
        let assessed = assess(
            &planned,
            evidence,
            &engine.engine_version,
            engine.engine_digest,
        )
        .unwrap();
        (planned, assessed)
    };
    assert_eq!(planned, expected_plan);
    assert_eq!(assessed, expected_assessment);
    let mut assessment_bytes = Vec::new();
    write_json(&assessed, &mut assessment_bytes, EXTERNAL_DOCUMENT_BYTES).unwrap();
    assert_eq!(
        assessment_bytes,
        serde_json_canonicalizer::to_vec(&expected_assessment).unwrap()
    );
    assert_eq!(parse_assessment(&assessment_bytes).unwrap(), assessed);
}

#[test]
fn bounded_output_counts_encoded_bytes_for_buffers_and_sinks() {
    let producer = ExternalEvidenceProducer {
        name: "probe \"quoted\" \\ \n \t é 😀".to_owned(),
        version: "0.0.0".to_owned(),
    };
    let expected = serde_json_canonicalizer::to_vec(&producer).unwrap();
    let length = u64::try_from(expected.len()).unwrap();
    for limit in [length, length + 1] {
        let mut bytes = Vec::new();
        let mut sink = std::io::sink();
        let outputs: [&mut dyn std::io::Write; 2] = [&mut bytes, &mut sink];
        for output in outputs {
            write_json(&producer, output, limit).unwrap();
        }
        assert_eq!(bytes, expected);
    }
    for limit in [0, length - 1] {
        let mut bytes = Vec::new();
        let mut sink = std::io::sink();
        let outputs: [&mut dyn std::io::Write; 2] = [&mut bytes, &mut sink];
        for output in outputs {
            let defect = write_json(&producer, output, limit).unwrap_err();
            assert_eq!(defect.kind, ErrorKind::LimitExceeded);
            assert_eq!(defect.path, "$");
        }
    }
    let mut short_output = [0; 8];
    let defect = write_json(
        &producer,
        std::io::Cursor::new(&mut short_output[..]),
        length,
    )
    .unwrap_err();
    assert_eq!(defect.kind, ErrorKind::InvalidValue);
    let mut planned = parse_plan(include_bytes!(
        "../../../../spec/examples/scanner-external-plan.json"
    ))
    .unwrap();
    planned.payload.retained_count = u64::MAX;
    let defect = write_json(&planned, std::io::sink(), EXTERNAL_DOCUMENT_BYTES).unwrap_err();
    assert_eq!(defect.kind, ErrorKind::InvalidValue);
}
