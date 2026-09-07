use amiss_wire::{
    digest::hb,
    external::{ExternalPlanEnvelope, PLAN_PAYLOAD_SCHEMA, parse_plan},
    report::model::{
        BaseSnapshot, Evaluation, ReportEnvelope, Snapshot, SnapshotUnavailableReason,
        UnavailableSnapshot, UnavailableSnapshotKind,
    },
    requests::{CandidateIdentity, RequestMode},
};

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-plan.json");

#[test]
fn external_snapshot_shapes_are_checked_even_with_matching_digests() {
    let document: ExternalPlanEnvelope = serde_json::from_slice(PLAN).unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap()).unwrap();
    let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap();
    let descriptor = &document.payload.report;
    let snapshots = [
        (
            "base",
            serde_json_canonicalizer::to_vec(&descriptor.base).unwrap(),
        ),
        (
            "candidate",
            serde_json_canonicalizer::to_vec(&descriptor.candidate).unwrap(),
        ),
    ];
    for (field, original) in snapshots {
        let original = String::from_utf8(original).unwrap();
        for replacement in [
            original.replace("git-commit", "future-snapshot"),
            original.replace("sha1", "unknown-object-format"),
            original.replace("\"commit_oid\"", "\"unknown_oid\""),
            original.replace("\"commit_oid\":\"", "\"commit_oid\":\"g"),
            original.replacen('{', "{\"future\":true,", 1),
            "{}".to_owned(),
            "null".to_owned(),
            "true".to_owned(),
            "42".to_owned(),
            "\"snapshot\"".to_owned(),
            "[]".to_owned(),
        ] {
            assert_ne!(original, replacement);
            let changed = payload.replace(
                &format!("\"{field}\":{original}"),
                &format!("\"{field}\":{replacement}"),
            );
            assert_ne!(changed, payload);
            let input = wire.replace(&payload, &changed).replace(
                &document.payload_digest.to_string(),
                &hb(PLAN_PAYLOAD_SCHEMA, changed.as_bytes()).to_string(),
            );
            let error = parse_plan(input.as_bytes()).unwrap_err();
            assert_eq!(
                error.path,
                format!("$.payload.report.{field}"),
                "{replacement}"
            );
        }
    }
    let invalid = payload.replace("\"mode\":\"commit-pair\"", "\"mode\":\"future-mode\"");
    assert_ne!(invalid, payload);
    let input = wire.replace(&payload, &invalid).replace(
        &document.payload_digest.to_string(),
        &hb(PLAN_PAYLOAD_SCHEMA, invalid.as_bytes()).to_string(),
    );
    assert!(parse_plan(input.as_bytes()).is_err());
}

#[test]
fn external_plans_retain_the_reports_existing_snapshot_types() {
    let bytes = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");
    let report: ReportEnvelope = serde_json::from_slice(bytes).unwrap();
    let generated = amiss_wire::external::plan(
        &report,
        &report.payload.engine.engine_version,
        report.payload.engine.engine_digest,
    )
    .unwrap();
    let plan = parse_plan(&generated).unwrap();
    let Evaluation::Resolved(evaluation) = report.payload.evaluation else {
        panic!("the example carries a resolved evaluation");
    };
    assert_eq!(plan.payload.report.base, evaluation.base);
    assert_eq!(plan.payload.report.candidate, evaluation.candidate);
    assert_eq!(plan.payload.report.mode, evaluation.mode);
    assert_eq!(plan.payload.report.payload_digest, report.payload_digest);
    assert_eq!(serde_json_canonicalizer::to_vec(&plan).unwrap(), generated);
}

#[test]
fn external_plan_snapshots_round_trip_without_positional_or_extra_fields() {
    let mut cases = Vec::new();
    for bytes in [
        include_bytes!("../../../../spec/examples/candidate-identity.json").as_slice(),
        include_bytes!("../../../../spec/examples/candidate-identity-index.json"),
    ] {
        let identity: CandidateIdentity = serde_json::from_slice(bytes).unwrap();
        cases.push((
            BaseSnapshot::Git(identity.base),
            Snapshot::Available(identity.candidate),
            identity.mode,
        ));
    }
    let unavailable = UnavailableSnapshot {
        kind: UnavailableSnapshotKind::Unavailable,
        reasons: vec![SnapshotUnavailableReason::NotSupplied],
        request_digest: None,
    };
    cases.push((
        BaseSnapshot::Unavailable(unavailable.clone()),
        Snapshot::Unavailable(unavailable),
        RequestMode::CommitPair,
    ));
    let mut document: ExternalPlanEnvelope = serde_json::from_slice(PLAN).unwrap();
    for (base, candidate, mode) in cases {
        document.payload.report.base = base;
        document.payload.report.candidate = candidate;
        document.payload.report.mode = mode;
        document.payload_digest = hb(
            PLAN_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
        );
        let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap();
        assert_eq!(parse_plan(wire.as_bytes()).unwrap(), document);
        let descriptor = &document.payload.report;
        let object = serde_json::to_string(descriptor).unwrap();
        for (offset, _) in object.match_indices('{') {
            let mut invalid = object.clone();
            invalid.insert_str(offset + 1, "\"future\":true,");
            assert!(
                serde_json::from_str::<amiss_wire::external::ExternalPlanReport>(&invalid).is_err(),
                "{invalid}"
            );
        }
        let positional = serde_json::to_string(&(
            &descriptor.payload_digest,
            &descriptor.base,
            &descriptor.candidate,
            descriptor.mode,
        ))
        .unwrap();
        let canonical =
            String::from_utf8(serde_json_canonicalizer::to_vec(descriptor).unwrap()).unwrap();
        let payload =
            String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap())
                .unwrap();
        let changed = payload.replace(&canonical, &positional);
        assert_ne!(changed, payload);
        let malformed = wire.replace(&payload, &changed).replace(
            &document.payload_digest.to_string(),
            &hb(PLAN_PAYLOAD_SCHEMA, changed.as_bytes()).to_string(),
        );
        let error = parse_plan(malformed.as_bytes()).unwrap_err();
        assert_eq!(error.path, "$.payload.report");
        assert_eq!(error.kind, amiss_wire::de::ErrorKind::WrongType);
    }
}
