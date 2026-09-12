use amiss_wire::{
    de::ErrorKind,
    digest::hb,
    external::{
        ExternalPlan, ExternalPlanEnvelope, ExternalRepository, PLAN_PAYLOAD_SCHEMA, PlanDefect,
        parse_plan, plan,
    },
    model::ForgeDialect,
    report::{PAYLOAD_SCHEMA, model::ReportStatus, validate_envelope},
};

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-plan.json");

#[test]
fn typed_plan_derivation_rechecks_integrity_before_result_laws() {
    let (report, _) = validate_envelope(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))
    .unwrap();
    let engine = &report.payload.engine;
    assert!(plan(&report, &engine.engine_version, engine.engine_digest).is_ok());

    let mut tampered = report.clone();
    tampered.payload.result.finding_count += 1;
    let mut invalid_result = report.clone();
    invalid_result.payload.result.status = ReportStatus::Incomplete;
    assert_eq!(
        plan(
            &invalid_result,
            &engine.engine_version,
            engine.engine_digest
        ),
        Err(PlanDefect::DigestMismatch)
    );
    invalid_result.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&invalid_result.payload).unwrap(),
    );
    let mut invalid_count = report.clone();
    invalid_count.payload.summary.findings.warn = u64::MAX;
    let mut incomplete = invalid_result.clone();
    incomplete.payload.result.complete = false;
    incomplete.payload.result.exit_code = 2;
    incomplete.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&incomplete.payload).unwrap(),
    );
    for (input, defect) in [
        (tampered, PlanDefect::DigestMismatch),
        (invalid_result, PlanDefect::InvalidResult),
        (invalid_count, PlanDefect::NotAReport),
        (incomplete, PlanDefect::Incomplete),
    ] {
        assert_eq!(
            plan(&input, &engine.engine_version, engine.engine_digest),
            Err(defect)
        );
    }
}

#[test]
fn external_plan_objects_reject_extra_fields_with_matching_digests() {
    let mut document: ExternalPlanEnvelope = serde_json::from_slice(PLAN).unwrap();
    document.payload.introduced[0].repository = Some(ExternalRepository {
        host: "github.com".to_owned(),
        dialect: ForgeDialect::Github,
        owner: "acme".to_owned(),
        name: "widgets".to_owned(),
        form: Some("blob".to_owned()),
        tail: Some("main/manual.md".to_owned()),
    });
    let mut removed = document.payload.introduced[0].clone();
    removed.destination = "https://example.com/removed".to_owned();
    document.payload.removed.push(removed);
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap()).unwrap();
    document.payload_digest = hb(PLAN_PAYLOAD_SCHEMA, payload.as_bytes());
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap();
    assert_eq!(parse_plan(wire.as_bytes()).unwrap(), document);

    let extended = wire.replacen('{', "{\"future\":true,", 1);
    let error = parse_plan(extended.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$");
    assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));

    for (offset, _) in payload.match_indices('{') {
        let mut extended = payload.clone();
        extended.insert_str(offset + 1, "\"future\":true,");
        let canonical = serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(
            &mut serde_json::Deserializer::from_str(&extended),
        ))
        .unwrap();
        let input = wire.replace(&payload, &extended).replace(
            &document.payload_digest.to_string(),
            &hb(PLAN_PAYLOAD_SCHEMA, &canonical).to_string(),
        );
        assert!(
            serde_json::from_str::<ExternalPlan>(&extended).is_err(),
            "the model accepted an extra field: {extended}"
        );
        let error = parse_plan(input.as_bytes()).unwrap_err();
        assert!(
            matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()),
            "{extended}"
        );
    }
}

#[test]
fn external_plan_ingress_refuses_positional_data_with_either_payload_digest() {
    let mut document: ExternalPlanEnvelope = serde_json::from_slice(PLAN).unwrap();
    document.payload.introduced[0].repository = Some(ExternalRepository {
        host: "github.com".to_owned(),
        dialect: ForgeDialect::Github,
        owner: "acme".to_owned(),
        name: "widgets".to_owned(),
        form: None,
        tail: None,
    });
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap()).unwrap();
    document.payload_digest = hb(PLAN_PAYLOAD_SCHEMA, payload.as_bytes());
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap();
    assert_eq!(parse_plan(wire.as_bytes()).unwrap(), document);
    let plan = &document.payload;
    let row = &plan.introduced[0];
    let repository = row.repository.as_ref().unwrap();
    for (original, positional) in [
        (
            serde_json_canonicalizer::to_vec(plan).unwrap(),
            serde_json::to_string(&(
                plan.schema,
                &plan.engine,
                &plan.report,
                &plan.introduced,
                &plan.removed,
                plan.retained_count,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(&plan.engine).unwrap(),
            serde_json::to_string(&(&plan.engine.engine_version, plan.engine.engine_digest))
                .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(row).unwrap(),
            serde_json::to_string(&(
                &row.destination,
                &row.scheme,
                &row.documents,
                &row.repository,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(repository).unwrap(),
            serde_json::to_string(&(
                &repository.host,
                repository.dialect,
                &repository.owner,
                &repository.name,
            ))
            .unwrap(),
        ),
    ] {
        let original = String::from_utf8(original).unwrap();
        let changed = payload.replace(&original, &positional);
        assert_ne!(changed, payload);
        let canonical = serde_json_canonicalizer::to_vec(&serde_transcode::Transcoder::new(
            &mut serde_json::Deserializer::from_str(&changed),
        ))
        .unwrap();
        for digest in [document.payload_digest, hb(PLAN_PAYLOAD_SCHEMA, &canonical)] {
            let input = wire
                .replace(&payload, &changed)
                .replace(&document.payload_digest.to_string(), &digest.to_string());
            assert!(parse_plan(input.as_bytes()).is_err(), "{input}");
        }
    }
}

#[test]
fn external_plan_ingress_preserves_equivalent_json_and_checks_actual_changes() {
    let document: ExternalPlanEnvelope = serde_json::from_slice(PLAN).unwrap();
    for input in [
        serde_json::to_string(&document).unwrap(),
        serde_json::to_string_pretty(&document).unwrap(),
        serde_json::to_string(&document)
            .unwrap()
            .replace("https://", "https:\\/\\/"),
        serde_json::to_string(&document)
            .unwrap()
            .replace("retained_count", "retained_\\u0063ount"),
    ] {
        assert_eq!(parse_plan(input.as_bytes()).unwrap(), document);
    }
    let mut changed = document.clone();
    changed.payload.retained_count += 1;
    let error = parse_plan(&serde_json::to_vec(&changed).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload_digest");
    assert!(matches!(error.kind, ErrorKind::DigestMismatch));
}
