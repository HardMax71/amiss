use amiss_wire::{
    digest::hb,
    external::{ExternalPlanEnvelope, PLAN_PAYLOAD_SCHEMA, parse_plan, plan},
    report::{
        PAYLOAD_SCHEMA,
        model::{Evaluation, ReportEnvelope, Snapshot},
    },
    requests::{CandidateIdentity, CandidateSnapshot, IndexSnapshotIdentity},
};

#[test]
fn index_entry_count_is_checked_through_its_shared_consumers() {
    let mut identity: CandidateIdentity = serde_json::from_str(include_str!(
        "../../../../spec/examples/candidate-identity-index.json"
    ))
    .unwrap();
    let CandidateSnapshot::Index(mut index) = identity.candidate.clone() else {
        panic!("the index example carries an index snapshot");
    };
    let mut report: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))
    .unwrap();
    let Evaluation::Resolved(mut evaluation) = report.payload.evaluation.clone() else {
        panic!("the report example has a resolved evaluation");
    };
    let engine = report.payload.engine.clone();
    let mut derived: ExternalPlanEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-external-plan.json"
    ))
    .unwrap();
    for count in [0, js_int::MAX_SAFE_UINT - 1, js_int::MAX_SAFE_UINT] {
        index.entry_count = count;
        identity.candidate = CandidateSnapshot::Index(index.clone());
        evaluation.candidate = Snapshot::Available(identity.candidate.clone());
        evaluation.mode = identity.mode;
        report.payload.evaluation = Evaluation::Resolved(evaluation.clone());
        report.payload_digest = hb(
            PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
        );
        assert_eq!(
            amiss_wire::report::validate_envelope(
                &serde_json_canonicalizer::to_vec(&report).unwrap()
            )
            .unwrap()
            .0,
            report
        );
        let bytes = plan(&report, &engine.engine_version, engine.engine_digest).unwrap();
        derived = parse_plan(&bytes).unwrap();
        assert_eq!(derived.payload.report.candidate, evaluation.candidate);
        assert_eq!(serde_json_canonicalizer::to_vec(&derived).unwrap(), bytes);
        assert_eq!(
            serde_json::from_slice::<CandidateIdentity>(&serde_json::to_vec(&identity).unwrap())
                .unwrap(),
            identity
        );
    }
    let encoded = serde_json::to_string(&index).unwrap();
    let count = format!("\"entry_count\":{}", index.entry_count);
    for invalid in [
        "9007199254740992",
        "18446744073709551615",
        "-1",
        "-0",
        "0.0",
        "0e0",
        "\"0\"",
        "null",
        "false",
        "[]",
        "{}",
    ] {
        let changed = encoded.replace(&count, &format!("\"entry_count\":{invalid}"));
        assert_ne!(changed, encoded);
        assert!(serde_json::from_str::<IndexSnapshotIdentity>(&changed).is_err());
        let candidate = serde_json::to_string(&identity)
            .unwrap()
            .replace(&encoded, &changed);
        assert!(serde_json::from_str::<CandidateIdentity>(&candidate).is_err());
    }
    for invalid in [js_int::MAX_SAFE_UINT + 1, u64::MAX] {
        index.entry_count = invalid;
        identity.candidate = CandidateSnapshot::Index(index.clone());
        evaluation.candidate = Snapshot::Available(identity.candidate.clone());
        report.payload.evaluation = Evaluation::Resolved(evaluation.clone());
        derived.payload.report.candidate = evaluation.candidate.clone();
        assert!(serde_json::to_vec(&index).is_err());
        assert!(serde_json_canonicalizer::to_vec(&identity).is_err());
        assert!(serde_json_canonicalizer::to_vec(&report).is_err());
        assert!(serde_json_canonicalizer::to_vec(&derived).is_err());
        assert!(plan(&report, &engine.engine_version, engine.engine_digest).is_err());
    }
}

#[test]
fn retained_counts_are_bounded_by_serde_before_plan_verification() {
    let mut document: ExternalPlanEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-external-plan.json"
    ))
    .unwrap();
    for retained_count in [0, js_int::MAX_SAFE_UINT - 1, js_int::MAX_SAFE_UINT] {
        document.payload.retained_count = retained_count;
        document.payload_digest = hb(
            PLAN_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
        );
        let bytes = serde_json_canonicalizer::to_vec(&document).unwrap();
        assert_eq!(parse_plan(&bytes).unwrap(), document);
        let wire = serde_json::to_string(&document).unwrap();
        for invalid in [js_int::MAX_SAFE_UINT + 1, u64::MAX] {
            let changed = wire.replace(
                &format!("\"retained_count\":{retained_count}"),
                &format!("\"retained_count\":{invalid}"),
            );
            assert_ne!(changed, wire);
            assert!(serde_json::from_str::<ExternalPlanEnvelope>(&changed).is_err());
        }
    }
    for invalid in [js_int::MAX_SAFE_UINT + 1, u64::MAX] {
        document.payload.retained_count = invalid;
        assert!(serde_json::to_vec(&document.payload).is_err());
        assert!(serde_json_canonicalizer::to_vec(&document).is_err());
    }
}
