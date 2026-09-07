use amiss_wire::{
    de::{Error, ErrorKind},
    digest::hb,
    external::{
        ASSESSMENT_PAYLOAD_SCHEMA, AssessDefect, AssessmentDefect, ExternalAssessment,
        ExternalAssessmentEnvelope, ExternalReason, ExternalVerdict, ExternalVerdictRow,
        PLAN_PAYLOAD_SCHEMA, assess, parse_assessment, parse_plan,
    },
};

const ASSESSMENT: &[u8] =
    include_bytes!("../../../../spec/examples/scanner-external-assessment.json");

#[test]
fn assessment_checks_mutable_plan_identity_and_laws_before_evidence() {
    let plan = parse_plan(include_bytes!(
        "../../../../spec/examples/scanner-external-plan.json"
    ))
    .unwrap();
    let evidence = include_bytes!("../../../../spec/examples/scanner-external-evidence.json");
    let engine = hb("test", b"engine");
    let assessment = assess(&plan, evidence, "0.0.0", engine).unwrap();
    assert_eq!(
        assessment.payload.subject.plan_payload_digest,
        plan.payload_digest
    );
    assert!(matches!(
        assess(&plan, b"null", "0.0.0", engine),
        Err(AssessDefect::Evidence(_))
    ));

    let mut changed = plan.clone();
    changed.payload.retained_count += 1;
    let mut invalid = plan.clone();
    invalid.payload.introduced[0].documents.clear();
    let mut rebound = invalid.clone();
    rebound.payload_digest = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&rebound.payload).unwrap(),
    );
    let mut out_of_range = plan;
    out_of_range.payload.retained_count = u64::MAX;
    for (input, path, kind) in [
        (&changed, "$.payload_digest", ErrorKind::DigestMismatch),
        (&invalid, "$.payload_digest", ErrorKind::DigestMismatch),
        (
            &rebound,
            "$.payload.introduced[0].documents",
            ErrorKind::InvalidValue,
        ),
        (&out_of_range, "$.payload", ErrorKind::InvalidValue),
    ] {
        let Err(AssessDefect::Plan(defect)) = assess(input, b"null", "0.0.0", engine) else {
            panic!("invalid plans must be refused before evidence is read");
        };
        assert_eq!(defect.path, path);
        assert_eq!(defect.kind, kind);
    }
    changed.payload_digest = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&changed.payload).unwrap(),
    );
    assert!(matches!(
        assess(&changed, evidence, "0.0.0", engine),
        Err(AssessDefect::UnboundEvidence)
    ));
}

#[test]
fn assessment_writer_uses_the_same_derived_validation_as_the_reader() {
    let plan = parse_plan(include_bytes!(
        "../../../../spec/examples/scanner-external-plan.json"
    ))
    .unwrap();
    assert!(matches!(
        assess(
            &plan,
            include_bytes!("../../../../spec/examples/scanner-external-evidence.json"),
            "",
            hb("test", b"engine"),
        ),
        Err(AssessDefect::Assessment(AssessmentDefect::Contract(_)))
    ));
}

#[test]
fn assessment_models_reject_extra_fields_with_matching_digests() {
    let mut document: ExternalAssessmentEnvelope = serde_json::from_slice(ASSESSMENT).unwrap();
    let row = document.payload.verdicts[0].clone();
    document.payload.verdicts = [
        (ExternalVerdict::Reachable, None),
        (ExternalVerdict::Refuted, Some(ExternalReason::Gone)),
        (ExternalVerdict::Unproven, Some(ExternalReason::Unexamined)),
    ]
    .map(|(verdict, reason)| ExternalVerdictRow {
        destination: format!("https://example.com/{verdict}"),
        verdict,
        reason,
        ..row.clone()
    })
    .into();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap()).unwrap();
    document.payload_digest = hb(ASSESSMENT_PAYLOAD_SCHEMA, payload.as_bytes());
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap();
    assert_eq!(parse_assessment(wire.as_bytes()).unwrap(), document);
    let extended = wire.replacen('{', "{\"future\":true,", 1);
    let Err(AssessmentDefect::Wire(error)) = parse_assessment(extended.as_bytes()) else {
        panic!("the envelope must reject extra fields");
    };
    assert_eq!(error.path, "$.future");
    assert_eq!(error.kind, ErrorKind::UnknownField);

    for (offset, _) in payload.match_indices('{') {
        let mut extended = payload.clone();
        extended.insert_str(offset + 1, "\"future\":true,");
        let canonical = serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(
            &mut serde_json::Deserializer::from_str(&extended),
        ))
        .unwrap();
        let changed = wire.replace(&payload, &extended).replace(
            &document.payload_digest.to_string(),
            &hb(ASSESSMENT_PAYLOAD_SCHEMA, &canonical).to_string(),
        );
        assert_eq!(
            [
                serde_json::from_str::<ExternalAssessment>(&extended).is_err(),
                matches!(
                    parse_assessment(changed.as_bytes()),
                    Err(AssessmentDefect::Wire(Error {
                        kind: ErrorKind::UnknownField,
                        ..
                    }))
                ),
            ],
            [true; 2],
            "{extended}"
        );
    }
}

#[test]
fn assessment_positional_forms_fail_with_original_and_rebound_digests() {
    let document: ExternalAssessmentEnvelope = serde_json::from_slice(ASSESSMENT).unwrap();
    let payload = &document.payload;
    let row = &payload.verdicts[0];
    let original = String::from_utf8(serde_json_canonicalizer::to_vec(payload).unwrap()).unwrap();
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap();
    for (object, positional) in [
        (
            serde_json_canonicalizer::to_vec(payload).unwrap(),
            serde_json::to_string(&(
                payload.schema,
                &payload.engine,
                payload.subject,
                &payload.producer,
                &payload.verdicts,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(&payload.engine).unwrap(),
            serde_json::to_string(&(&payload.engine.engine_version, payload.engine.engine_digest))
                .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(&payload.subject).unwrap(),
            serde_json::to_string(&(
                payload.subject.report_payload_digest,
                payload.subject.plan_payload_digest,
                payload.subject.evidence_digest,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(&payload.producer).unwrap(),
            serde_json::to_string(&(&payload.producer.name, &payload.producer.version)).unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(row).unwrap(),
            serde_json::to_string(&(
                &row.destination,
                &row.documents,
                row.verdict,
                row.reason,
                &row.retarget,
            ))
            .unwrap(),
        ),
    ] {
        let object = String::from_utf8(object).unwrap();
        let changed = original.replace(&object, &positional);
        assert_ne!(changed, original);
        let canonical = serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(
            &mut serde_json::Deserializer::from_str(&changed),
        ))
        .unwrap();
        for digest in [
            document.payload_digest,
            hb(ASSESSMENT_PAYLOAD_SCHEMA, &canonical),
        ] {
            let malformed = wire
                .replace(&original, &changed)
                .replace(&document.payload_digest.to_string(), &digest.to_string());
            assert!(
                parse_assessment(malformed.as_bytes()).is_err(),
                "{malformed}"
            );
        }
    }
    let positional =
        serde_json::to_vec(&(document.schema, &document.payload, document.payload_digest)).unwrap();
    assert!(matches!(
        parse_assessment(&positional),
        Err(AssessmentDefect::Wire(Error {
            kind: ErrorKind::WrongType,
            ..
        }))
    ));
}

#[test]
fn assessments_keep_derived_errors_before_digest_mismatches() {
    let document: ExternalAssessmentEnvelope = serde_json::from_slice(ASSESSMENT).unwrap();
    let mut wrong_reason = document.clone();
    wrong_reason.payload.verdicts[0].verdict = ExternalVerdict::Reachable;
    let mut repeated_destination = document.clone();
    let mut other = repeated_destination.payload.verdicts[0].clone();
    other.documents[0] = "docs/other.md".to_owned();
    repeated_destination.payload.verdicts.push(other);
    let mut repeated_document = document.clone();
    let other = repeated_document.payload.verdicts[0].documents[0].clone();
    repeated_document.payload.verdicts[0].documents.push(other);
    let mut empty_retarget = document.clone();
    empty_retarget.payload.verdicts[0].retarget = Some(String::new());
    for mut invalid in [
        wrong_reason,
        repeated_destination,
        repeated_document,
        empty_retarget,
    ] {
        let stale = serde_json::to_vec(&invalid).unwrap();
        invalid.payload_digest = hb(
            ASSESSMENT_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&invalid.payload).unwrap(),
        );
        for bytes in [stale, serde_json::to_vec(&invalid).unwrap()] {
            assert!(matches!(
                parse_assessment(&bytes),
                Err(AssessmentDefect::Contract(_))
            ));
        }
    }
    let row = &document.payload.verdicts[0];
    let wire = serde_json::to_string(&document).unwrap();
    for (field, value) in [
        ("reason", serde_json::to_string(&row.reason).unwrap()),
        ("retarget", serde_json::to_string(&row.retarget).unwrap()),
    ] {
        let changed = wire.replace(
            &format!("\"{field}\":{value}"),
            &format!("\"{field}\":null"),
        );
        assert_ne!(changed, wire);
        assert!(matches!(
            parse_assessment(changed.as_bytes()),
            Err(AssessmentDefect::Wire(Error {
                kind: ErrorKind::WrongType,
                ..
            }))
        ));
    }
}

#[test]
fn assessment_identity_survives_formatting_but_not_changed_subjects() {
    let mut document: ExternalAssessmentEnvelope = serde_json::from_slice(ASSESSMENT).unwrap();
    for text in [
        serde_json::to_string(&document).unwrap(),
        serde_json::to_string_pretty(&document).unwrap(),
        serde_json::to_string(&document)
            .unwrap()
            .replace("https://", "https:\\/\\/")
            .replace("verdicts", "ver\\u0064icts"),
    ] {
        assert_eq!(parse_assessment(text.as_bytes()).unwrap(), document);
    }
    document.payload.subject.evidence_digest = hb("test", b"changed evidence");
    assert!(matches!(
        parse_assessment(&serde_json::to_vec(&document).unwrap()),
        Err(AssessmentDefect::Wire(Error {
            kind: ErrorKind::DigestMismatch,
            ..
        }))
    ));
    document.payload_digest = hb(
        ASSESSMENT_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
    );
    assert_eq!(
        parse_assessment(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap(),
        document
    );
}
