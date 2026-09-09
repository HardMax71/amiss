use amiss_wire::{
    de::{Error, ErrorKind},
    digest::hb,
    external::{self, AssessmentDefect, ExternalAssessmentEnvelope, ExternalPlanEnvelope},
};

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-plan.json");
const ASSESSMENT: &[u8] =
    include_bytes!("../../../../spec/examples/scanner-external-assessment.json");
const EVIDENCE: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-evidence.json");

#[test]
fn external_envelopes_keep_strict_inputs_and_complete_payload_digests() {
    let readers: [fn(&[u8]) -> bool; 3] = [
        |bytes| external::parse_plan(bytes).is_ok(),
        |bytes| external::parse_assessment(bytes).is_ok(),
        |bytes| external::parse_evidence(bytes).is_ok(),
    ];
    for ((bytes, schema), read) in [
        (PLAN, external::PLAN_ENVELOPE_SCHEMA),
        (ASSESSMENT, external::ASSESSMENT_ENVELOPE_SCHEMA),
        (EVIDENCE, external::EVIDENCE_SCHEMA),
    ]
    .into_iter()
    .zip(readers)
    {
        assert!(read(bytes));
        let text = std::str::from_utf8(bytes).unwrap();
        for key in ["schema", r"\u0073chema"] {
            let duplicate = text.replacen('{', &format!(r#"{{"{key}":"{schema}","#), 1);
            assert!(!read(duplicate.as_bytes()), "{duplicate}");
        }
        for invalid in [b"null".as_slice(), b"true", b"0", b"[]", b"\xff"] {
            assert!(!read(invalid));
        }
        assert!(!read(format!("\u{feff}{text}").as_bytes()));
        assert!(read(format!(" \n{text}\r\t").as_bytes()));
        for member in [
            r#""future":-0,"#,
            r#""future":0.5,"#,
            r#""future":1e0,"#,
            r#""future":9007199254740992,"#,
            r#""future":0,"future":1,"#,
            r#""future":0,"\u0066uture":1,"#,
        ] {
            let invalid = text.replacen('{', &format!("{{{member}"), 1);
            assert!(!read(invalid.as_bytes()), "{member}");
        }
        for suffix in ["null", "{}", "garbage"] {
            assert!(!read(format!("{text}{suffix}").as_bytes()));
        }
        assert!(!read(format!("[{text}]").as_bytes()));
        let oversized = vec![b' '; usize::try_from(external::EXTERNAL_DOCUMENT_BYTES + 1).unwrap()];
        assert!(!read(&oversized));
    }

    let mut assessment: ExternalAssessmentEnvelope = serde_json::from_slice(ASSESSMENT).unwrap();
    assessment
        .payload
        .engine
        .engine_version
        .push_str("-changed");
    assert!(matches!(
        external::parse_assessment(&serde_json::to_vec(&assessment).unwrap()),
        Err(AssessmentDefect::Wire(Error {
            kind: ErrorKind::DigestMismatch,
            ..
        }))
    ));
    assessment.payload_digest = hb(
        external::ASSESSMENT_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&assessment.payload).unwrap(),
    );
    assert_eq!(
        external::parse_assessment(&serde_json::to_vec(&assessment).unwrap()).unwrap(),
        assessment
    );
}

#[test]
fn external_payloads_keep_structural_paths_and_semantic_validation_order() {
    let mut plan: ExternalPlanEnvelope = serde_json::from_slice(PLAN).unwrap();
    assert!(external::validate_plan_envelope(&plan).is_ok());
    let version = serde_json::to_string(&plan.payload.engine.engine_version).unwrap();
    let wire = serde_json::to_string(&plan).unwrap();
    let malformed = wire.replace(
        &format!("\"engine_version\":{version}"),
        "\"engine_version\":1",
    );
    assert_ne!(malformed, wire);
    let defect = external::parse_plan(malformed.as_bytes()).unwrap_err();
    assert_eq!(defect.path, "$.payload.engine.engine_version");
    assert_eq!(defect.kind, ErrorKind::WrongType);
    plan.payload.engine.engine_version.clear();
    let defect = external::validate_plan_envelope(&plan).unwrap_err();
    assert_eq!(defect.path, "$.payload_digest");
    assert_eq!(defect.kind, ErrorKind::DigestMismatch);
    assert_eq!(
        external::parse_plan(&serde_json::to_vec(&plan).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::DigestMismatch
    );
    plan.payload_digest = hb(
        external::PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&plan.payload).unwrap(),
    );
    let defect = external::validate_plan_envelope(&plan).unwrap_err();
    assert_eq!(defect.path, "$.payload.engine.engine_version");
    assert_eq!(defect.kind, ErrorKind::InvalidValue);
    assert_eq!(
        external::parse_plan(&serde_json::to_vec(&plan).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
    plan.payload.retained_count = u64::MAX;
    let defect = external::validate_plan_envelope(&plan).unwrap_err();
    assert_eq!(defect.path, "$.payload");
    assert_eq!(defect.kind, ErrorKind::InvalidValue);

    let mut assessment: ExternalAssessmentEnvelope = serde_json::from_slice(ASSESSMENT).unwrap();
    let version = serde_json::to_string(&assessment.payload.producer.version).unwrap();
    let wire = serde_json::to_string(&assessment).unwrap();
    let malformed = wire.replace(&format!("\"version\":{version}"), "\"version\":null");
    assert_ne!(malformed, wire);
    let Err(AssessmentDefect::Wire(defect)) = external::parse_assessment(malformed.as_bytes())
    else {
        panic!("the producer version must be a string");
    };
    assert_eq!(defect.path, "$.payload.producer.version");
    assert_eq!(defect.kind, ErrorKind::WrongType);
    assessment.payload.producer.version.clear();
    assert!(matches!(
        external::parse_assessment(&serde_json::to_vec(&assessment).unwrap()),
        Err(AssessmentDefect::Contract(_))
    ));
}

#[test]
fn assessments_share_the_closed_engine_and_producer_descriptors() {
    let document: ExternalAssessmentEnvelope = serde_json::from_slice(ASSESSMENT).unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap()).unwrap();
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap();
    for field in ["engine", "producer"] {
        let extended = payload.replace(
            &format!("\"{field}\":{{"),
            &format!("\"{field}\":{{\"future\":true,"),
        );
        assert_ne!(extended, payload);
        let canonical = serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(
            &mut serde_json::Deserializer::from_str(&extended),
        ))
        .unwrap();
        let changed = wire.replace(&payload, &extended).replace(
            &document.payload_digest.to_string(),
            &hb(external::ASSESSMENT_PAYLOAD_SCHEMA, &canonical).to_string(),
        );
        let Err(AssessmentDefect::Wire(error)) = external::parse_assessment(changed.as_bytes())
        else {
            panic!("the shared {field} descriptor must reject unknown fields");
        };
        assert_eq!(error.path, format!("$.payload.{field}.future"));
        assert_eq!(error.kind, ErrorKind::UnknownField);
    }
}

#[test]
fn closed_external_plans_reject_invalid_numbers_and_deep_unknown_fields() {
    let document: ExternalPlanEnvelope = serde_json::from_slice(PLAN).unwrap();
    let wire = serde_json::to_string(&document).unwrap();
    for (member, expected, path) in [
        (
            "\"retained_count\":-0",
            ErrorKind::WrongType,
            "$.payload.retained_count",
        ),
        (
            "\"retained_count\":0.0",
            ErrorKind::WrongType,
            "$.payload.retained_count",
        ),
        (
            "\"retained_count\":0e0",
            ErrorKind::WrongType,
            "$.payload.retained_count",
        ),
        (
            "\"retained_count\":9007199254740992",
            ErrorKind::InvalidValue,
            "$.payload.retained_count",
        ),
        (
            "\"retained_count\":0,\"retained_count\":0",
            ErrorKind::InvalidValue,
            "$.payload",
        ),
        (
            "\"retained_count\":0,\"retained_\\u0063ount\":0",
            ErrorKind::InvalidValue,
            "$.payload",
        ),
    ] {
        let changed = wire.replace("\"retained_count\":0", member);
        assert_ne!(changed, wire);
        let defect = external::parse_plan(changed.as_bytes()).unwrap_err();
        assert_eq!(defect.kind, expected, "{member}: {defect:?}");
        assert_eq!(defect.path, path, "{member}");
    }
    let nested = format!("{}null{}", "[".repeat(510), "]".repeat(510));
    let changed = wire.replace(
        "\"payload\":{",
        &format!("\"payload\":{{\"future\":{nested},"),
    );
    assert_eq!(
        external::parse_plan(changed.as_bytes()).unwrap_err().kind,
        ErrorKind::UnknownField
    );
    let too_deep = changed.replace(&nested, &format!("[{nested}]"));
    let defect = external::parse_plan(too_deep.as_bytes()).unwrap_err();
    assert_eq!(defect.kind, ErrorKind::UnknownField);
    assert_eq!(defect.path, "$.payload.future");
}
