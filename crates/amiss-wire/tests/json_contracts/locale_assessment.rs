use amiss_wire::{
    assessment::Nullable,
    de::ErrorKind,
    digest::sha256,
    locale::{
        self, ASSESSMENT_DOCUMENT_BYTES, LocaleCoverageVerdict, LocaleFallbackStatus,
        LocaleLineageStatus,
    },
    semantic::PRODUCER_VERSION_BYTES,
};

use super::input::assert_object_required;

const ASSESSMENT: &[u8] =
    include_bytes!("../../../../spec/examples/locale-coverage-assessment.json");
const PLAN: &[u8] = include_bytes!("../../../../spec/examples/locale-coverage-plan.json");

#[test]
fn typed_locale_assessment_retains_full_results_and_bounded_output() {
    let plan = locale::parse_plan(PLAN).unwrap();
    let evidence = locale::parse_evidence(include_bytes!(
        "../../../../spec/examples/locale-coverage-evidence.json"
    ))
    .unwrap();
    let version = "a".repeat(PRODUCER_VERSION_BYTES);
    let engine = sha256(b"locale evaluator");
    let assessment = locale::assess(&plan, Some(&evidence), &version, engine).unwrap();
    assert_eq!(assessment.payload.engine.engine_version, version);
    assert_eq!(assessment.payload.engine.engine_digest, engine);
    assert_eq!(assessment.payload.verdict, LocaleCoverageVerdict::Matched);
    let coverage = &assessment.payload.coverage;
    assert!(coverage.complete);
    assert_eq!(coverage.fallbacks.len(), 1);
    assert_eq!(coverage.fallbacks[0].status, LocaleFallbackStatus::Allowed);
    assert_eq!(coverage.lineage.len(), 1);
    assert_eq!(coverage.lineage[0].status, LocaleLineageStatus::Current);
    let Nullable::Value(product) = &assessment.payload.product else {
        panic!("the selected product must remain in the typed result");
    };
    assert_eq!(product.source, LocaleCoverageVerdict::Matched);
    assert_eq!(product.target, LocaleCoverageVerdict::Matched);
    let mut bytes = Vec::new();
    amiss_wire::write_json(&assessment, &mut bytes, ASSESSMENT_DOCUMENT_BYTES).unwrap();
    assert_eq!(locale::parse_assessment(&bytes).unwrap(), assessment);
    let exact = u64::try_from(bytes.len()).unwrap();
    assert!(exact <= ASSESSMENT_DOCUMENT_BYTES);
    amiss_wire::write_json(&assessment, std::io::sink(), exact).unwrap();
    assert_eq!(
        amiss_wire::write_json(&assessment, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    for invalid in [String::new(), format!("{version}a"), "1 bad".to_owned()] {
        let error = locale::assess(&plan, Some(&evidence), &invalid, engine).unwrap_err();
        assert_eq!(error.path, "$.payload.engine.engine_version");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }
}

#[test]
fn locale_assessment_requires_objects_and_schema_tags() -> Result<(), Box<dyn std::error::Error>> {
    let document = locale::parse_assessment(ASSESSMENT)?;
    let input = (&document, locale::parse_assessment, ErrorKind::WrongType);
    let payload = &document.payload;
    let coverage = &payload.coverage;
    let fallback = &coverage.fallbacks[0];
    let lineage = &coverage.lineage[0];
    let Nullable::Value(product) = &payload.product else {
        panic!("the committed example includes the selected product");
    };
    assert_object_required(
        input,
        &document,
        (document.schema, payload, document.payload_digest),
    )?;
    assert_object_required(
        input,
        payload,
        (
            payload.schema,
            &payload.engine,
            &payload.subject,
            payload.verdict,
            &payload.reasons,
            coverage,
            &payload.product,
        ),
    )?;
    assert_object_required(
        input,
        &payload.engine,
        (&payload.engine.engine_version, payload.engine.engine_digest),
    )?;
    assert_object_required(
        input,
        &payload.subject,
        (
            payload.subject.report_payload_digest,
            payload.subject.plan_payload_digest,
            &payload.subject.evidence_payload_digest,
        ),
    )?;
    assert_object_required(
        input,
        coverage,
        (
            coverage.complete,
            &coverage.source_missing,
            &coverage.target_missing,
            &coverage.target_orphaned,
            &coverage.fallbacks,
            &coverage.lineage,
        ),
    )?;
    assert_object_required(
        input,
        fallback,
        (&fallback.key, &fallback.class, fallback.status),
    )?;
    assert_object_required(input, lineage, (&lineage.key, lineage.status))?;
    assert_object_required(input, product, (product.source, product.target))?;

    let text = serde_json::to_string(&document)?;
    for (tag, path) in [
        (serde_json::to_string(&document.schema)?, "$.schema"),
        (serde_json::to_string(&payload.schema)?, "$.payload.schema"),
    ] {
        for (invalid, kind) in [
            ("null", ErrorKind::WrongType),
            ("false", ErrorKind::WrongType),
            (r#""unknown""#, ErrorKind::InvalidValue),
        ] {
            let changed = text.replacen(&tag, invalid, 1);
            assert_ne!(changed, text);
            let error = locale::parse_assessment(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, path);
            assert_eq!(error.kind, kind);
        }
        let missing = text.replacen(&format!("\"schema\":{tag},"), "", 1);
        assert_ne!(missing, text);
        let error = locale::parse_assessment(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
    Ok(())
}

#[test]
fn nullable_locale_assessment_fields_remain_required_with_precise_errors() {
    let plan = locale::parse_plan(PLAN).unwrap();
    let document = locale::assess(&plan, None, "1", sha256(b"engine")).unwrap();
    let text = serde_json::to_string(&document).unwrap();
    assert_eq!(locale::parse_assessment(text.as_bytes()).unwrap(), document);
    for (field, path) in [
        ("product", "$.payload.product"),
        (
            "evidence_payload_digest",
            "$.payload.subject.evidence_payload_digest",
        ),
    ] {
        let member = format!(",\"{field}\":null");
        assert_eq!(text.matches(&member).count(), 1);
        let missing = text.replacen(&member, "", 1);
        assert_ne!(missing, text);
        assert!(
            serde_json::from_str::<locale::LocaleCoverageAssessmentEnvelope>(&missing).is_err()
        );
        let error = locale::parse_assessment(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
}
