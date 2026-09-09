use amiss_wire::{
    de::ErrorKind,
    relation::{self, RELATION_DOCUMENT_BYTES, RelationVerdict},
    semantic::PRODUCER_VERSION_BYTES,
};

const ASSESSMENT: &[u8] = include_bytes!("../../../../spec/examples/relation-assessment.json");

#[test]
fn typed_relation_assessment_preserves_the_committed_document_and_output_limit() {
    let plan = relation::parse_plan(include_bytes!(
        "../../../../spec/examples/relation-plan.json"
    ))
    .unwrap();
    let evidence = relation::parse_evidence(include_bytes!(
        "../../../../spec/examples/relation-evidence.json"
    ))
    .unwrap();
    let published = relation::parse_assessment(ASSESSMENT).unwrap();
    let engine = &published.payload.engine;
    let replayed = relation::assess(
        &plan,
        Some(&evidence),
        &engine.engine_version,
        engine.engine_digest,
    )
    .unwrap();
    assert_eq!(replayed, published);
    let mut input = serde_json::Deserializer::from_slice(ASSESSMENT);
    assert_eq!(
        serde_json_canonicalizer::to_vec(&replayed).unwrap(),
        serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(&mut input)).unwrap()
    );
    input.end().unwrap();

    let version = "a".repeat(PRODUCER_VERSION_BYTES);
    let assessment =
        relation::assess(&plan, Some(&evidence), &version, engine.engine_digest).unwrap();
    assert_eq!(assessment.payload.engine.engine_version, version);
    assert_eq!(assessment.payload.verdict, RelationVerdict::IntroducedDrift);
    let mut bytes = Vec::new();
    amiss_wire::write_json(&assessment, &mut bytes, RELATION_DOCUMENT_BYTES).unwrap();
    assert_eq!(relation::parse_assessment(&bytes).unwrap(), assessment);
    let exact = u64::try_from(bytes.len()).unwrap();
    assert!(exact <= RELATION_DOCUMENT_BYTES);
    amiss_wire::write_json(&assessment, std::io::sink(), exact).unwrap();
    assert_eq!(
        amiss_wire::write_json(&assessment, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    for invalid in [String::new(), format!("{version}a"), "1 bad".to_owned()] {
        let error =
            relation::assess(&plan, Some(&evidence), &invalid, engine.engine_digest).unwrap_err();
        assert_eq!(error.path, "$.payload.engine.engine_version");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }
}

#[test]
fn relation_assessment_schema_tags_and_object_shapes_are_closed() {
    let document = relation::parse_assessment(ASSESSMENT).unwrap();
    let text = serde_json::to_string(&document).unwrap();
    let payload = &document.payload;
    let engine = &payload.engine;
    let subject = &payload.subject;
    for (schema, path) in [
        (serde_json::to_string(&document.schema).unwrap(), "$.schema"),
        (
            serde_json::to_string(&payload.schema).unwrap(),
            "$.payload.schema",
        ),
    ] {
        for (invalid, kind) in [
            ("null", ErrorKind::WrongType),
            ("[]", ErrorKind::WrongType),
            (r#""unknown""#, ErrorKind::InvalidValue),
        ] {
            let changed = text.replacen(&schema, invalid, 1);
            assert_ne!(changed, text);
            let error = relation::parse_assessment(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, path);
            assert_eq!(error.kind, kind);
        }
        let missing = text.replacen(&format!("\"schema\":{schema},"), "", 1);
        assert_ne!(missing, text);
        let error = relation::parse_assessment(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
    for (object, positional, path) in [
        (
            text.clone(),
            serde_json::to_string(&(document.schema, payload, document.payload_digest)).unwrap(),
            "$",
        ),
        (
            serde_json::to_string(payload).unwrap(),
            serde_json::to_string(&(
                payload.schema,
                engine,
                subject,
                payload.verdict,
                payload.reason,
            ))
            .unwrap(),
            "$.payload",
        ),
        (
            serde_json::to_string(engine).unwrap(),
            serde_json::to_string(&(&engine.engine_version, engine.engine_digest)).unwrap(),
            "$.payload.engine",
        ),
        (
            serde_json::to_string(subject).unwrap(),
            serde_json::to_string(&(
                subject.report_payload_digest,
                subject.plan_payload_digest,
                subject.evidence_payload_digest,
            ))
            .unwrap(),
            "$.payload.subject",
        ),
    ] {
        for (replacement, path, kind) in [
            (positional, path.to_owned(), ErrorKind::WrongType),
            (
                object.replacen('{', r#"{"unknown":true,"#, 1),
                format!("{path}.unknown"),
                ErrorKind::UnknownField,
            ),
        ] {
            let changed = text.replacen(&object, &replacement, 1);
            assert_ne!(changed, text);
            let error = relation::parse_assessment(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, path);
            assert_eq!(error.kind, kind);
        }
    }
}
