use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::digest::{hb, hj_serde};
use amiss_wire::report::{PAYLOAD_SCHEMA, model};
use amiss_wire::requests::CandidateSnapshot;

use amiss_fixtures::corrupt;

use super::accepted_report;

mod positional;

#[test]
fn report_rows_are_decoded_not_just_counted() {
    let (wire, expectations) = accepted_report();
    let report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    for (name, rows, count) in [
        (
            "documents",
            serde_json_canonicalizer::to_string(&report.payload.documents).unwrap(),
            report.payload.documents.len(),
        ),
        (
            "observations",
            serde_json_canonicalizer::to_string(&report.payload.observations).unwrap(),
            report.payload.observations.len(),
        ),
        (
            "findings",
            serde_json_canonicalizer::to_string(&report.payload.findings).unwrap(),
            report.payload.findings.len(),
        ),
        (
            "errors",
            serde_json_canonicalizer::to_string(&report.payload.errors).unwrap(),
            report.payload.errors.len(),
        ),
    ] {
        let original = format!(r#""{name}":{rows}"#);
        let invalid = format!(r#""{name}":[{}]"#, vec!["null"; count.max(1)].join(","));
        assert_eq!(
            accept(
                &corrupt(&report, &original, &invalid).unwrap(),
                &expectations
            ),
            Err(AcceptanceDefect::Shape),
            "{name}"
        );
    }
}

#[test]
fn typed_normalization_cannot_substitute_for_the_input_bytes() {
    use amiss_wire::report::model::ReportEnvelope;

    let (wire, expectations) = accepted_report();
    let report: ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let step = &report.payload.findings[0].policy_trace[0];
    let object = String::from_utf8(serde_json_canonicalizer::to_vec(step).unwrap()).unwrap();
    let sequence =
        serde_json::to_string(&(step.after, step.before, &step.rule_id, step.source)).unwrap();
    let original = String::from_utf8(wire).unwrap();
    let changed = original.replace(&object, &sequence);
    assert_ne!(changed, original);
    assert_eq!(
        serde_json::from_str::<ReportEnvelope>(&changed).unwrap(),
        report
    );
    assert_eq!(
        accept(changed.as_bytes(), &expectations),
        Err(AcceptanceDefect::Noncanonical)
    );
}

#[test]
fn typed_counts_still_obey_the_strict_json_integer_limit() {
    let (wire, expectations) = accepted_report();
    let mut report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    report.payload.summary.findings.warn = 9_007_199_254_740_991;
    let original = serde_json_canonicalizer::to_string(&report.payload.summary.findings).unwrap();
    let invalid = original.replacen("9007199254740991", "9007199254740992", 1);
    let wire = corrupt(&report, &original, &invalid).unwrap();
    assert!(serde_json::from_slice::<model::ReportEnvelope>(&wire).is_err());
    assert_eq!(accept(&wire, &expectations), Err(AcceptanceDefect::Shape));
}

#[test]
fn report_readers_agree_on_complete_status_and_exit_code() {
    use amiss_wire::report::{
        ReportDefect,
        model::{ReportEnvelope, ReportStatus},
        validate_envelope,
    };

    let (wire, expectations) = accepted_report();
    let mut report: ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let valid = [
        (true, ReportStatus::Pass, 0),
        (true, ReportStatus::Fail, 1),
        (false, ReportStatus::Incomplete, 2),
    ];
    for complete in [false, true] {
        for status in [
            ReportStatus::Pass,
            ReportStatus::Fail,
            ReportStatus::Incomplete,
        ] {
            for exit_code in [0, 1, 2, 3, u8::MAX] {
                report.payload.result.complete = complete;
                report.payload.result.status = status;
                report.payload.result.exit_code = exit_code;
                let bytes = bind(&mut report);
                let (normal, sealed) = if valid.contains(&(complete, status, exit_code)) {
                    (Ok(exit_code), Ok(i64::from(exit_code)))
                } else {
                    (
                        Err(ReportDefect::InvalidResult),
                        Err(AcceptanceDefect::Completeness),
                    )
                };
                assert_eq!(
                    validate_envelope(&bytes).map(|(_, verdict)| verdict.code()),
                    normal,
                    "{complete} {status:?} {exit_code}"
                );
                assert_eq!(
                    accept(&bytes, &expectations),
                    sealed,
                    "{complete} {status:?} {exit_code}"
                );
            }
        }
    }
}

#[test]
fn report_result_members_are_required_and_typed_in_both_readers() {
    use amiss_wire::report::{ReportDefect, model::ReportEnvelope, validate_envelope};

    let (wire, expectations) = accepted_report();
    let report: ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let result =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload.result).unwrap())
            .unwrap();
    let error_count = format!("\"error_count\":{}", report.payload.result.error_count);
    let finding_count = format!("\"finding_count\":{}", report.payload.result.finding_count);
    let omitted_error_count = format!("{error_count},");
    let omitted_finding_count = format!("{finding_count},");
    for (original, replacement) in [
        ("\"complete\":true,", ""),
        (omitted_error_count.as_str(), ""),
        ("\"exit_code\":0,", ""),
        (omitted_finding_count.as_str(), ""),
        (",\"status\":\"pass\"", ""),
        (error_count.as_str(), "\"error_count\":-1"),
        (error_count.as_str(), "\"error_count\":null"),
        (finding_count.as_str(), "\"finding_count\":-1"),
        (finding_count.as_str(), "\"finding_count\":null"),
        ("\"exit_code\":0", "\"exit_code\":256"),
        ("\"exit_code\":0", "\"exit_code\":-1"),
        ("\"status\":\"pass\"", "\"status\":\"unknown\""),
        (result.as_str(), "[true,0,0,0,\"pass\"]"),
    ] {
        let invalid = result.replace(original, replacement);
        assert_ne!(invalid, result, "{original}");
        let altered = corrupt(&report, &result, &invalid).unwrap();
        assert_eq!(
            validate_envelope(&altered).map(drop),
            Err(ReportDefect::NotAReport),
            "{invalid}"
        );
        assert_eq!(
            accept(&altered, &expectations),
            Err(AcceptanceDefect::Shape),
            "{invalid}"
        );
    }
}

#[test]
fn core_defects_keep_their_order_when_later_fields_are_also_wrong() {
    let (wire, expectations) = accepted_report();
    let original: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let wrong_digest = format!("sha256:{}", "0".repeat(64)).parse().unwrap();
    let mut report = original.clone();
    report.payload.result.finding_count = 1;
    let mut cases = vec![(report.clone(), AcceptanceDefect::FindingCount)];
    report.payload.result.complete = false;
    cases.push((report.clone(), AcceptanceDefect::Completeness));
    let model::Evaluation::Resolved(evaluation) = &mut report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let model::Snapshot::Available(CandidateSnapshot::Git(candidate)) = &mut evaluation.candidate
    else {
        panic!("the fixture has a Git candidate");
    };
    candidate.commit_oid = "b".repeat(40).parse().unwrap();
    cases.push((report.clone(), AcceptanceDefect::CandidateIdentity));
    let model::Evaluation::Resolved(evaluation) = &mut report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let model::BaseSnapshot::Git(base) = &mut evaluation.base else {
        panic!("the fixture has a Git base");
    };
    base.commit_oid = "a".repeat(40).parse().unwrap();
    cases.push((report.clone(), AcceptanceDefect::BaseIdentity));
    report.payload.engine.engine_digest = wrong_digest;
    cases.push((report, AcceptanceDefect::Engine));
    for (mut report, expected) in cases {
        assert_eq!(accept(&bind(&mut report), &expectations), Err(expected));
    }
    let mut report = original;
    report.payload.engine.engine_digest = wrong_digest;
    assert_eq!(
        accept(&bind(&mut report), &expectations),
        Err(AcceptanceDefect::Engine)
    );
    report.payload_digest = wrong_digest;
    let mut wire = serde_json_canonicalizer::to_string(&report).unwrap();
    wire.push('\n');
    assert_eq!(
        accept(wire.as_bytes(), &expectations),
        Err(AcceptanceDefect::PayloadDigest)
    );
    let result = serde_json_canonicalizer::to_string(&report.payload.result).unwrap();
    assert_eq!(wire.matches(&result).count(), 1);
    let malformed = wire.replacen(&result, "null", 1);
    assert_eq!(
        accept(malformed.as_bytes(), &expectations),
        Err(AcceptanceDefect::Shape)
    );
}

#[test]
fn core_status_tags_are_strings_and_completion_is_boolean() {
    let (wire, expectations) = accepted_report();
    let mut report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let candidate = serde_json_canonicalizer::to_string(&evaluation.candidate).unwrap();
    let evaluation = serde_json_canonicalizer::to_string(evaluation).unwrap();
    for (original, invalid) in [
        (
            evaluation.clone(),
            evaluation.replacen('{', r#"{"status":{"unavailable":null},"#, 1),
        ),
        (
            candidate.clone(),
            candidate.replacen(r#""kind":"git-commit""#, r#""kind":{"git-commit":null}"#, 1),
        ),
    ] {
        assert_eq!(
            accept(
                &corrupt(&report, &original, &invalid).unwrap(),
                &expectations
            ),
            Err(AcceptanceDefect::Shape)
        );
    }
    report.payload.result.exit_code = 2;
    report.payload.result.status = model::ReportStatus::Incomplete;
    report.payload.result.complete = false;
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(2));
    let result = serde_json_canonicalizer::to_string(&report.payload.result).unwrap();
    for invalid in ["null", r#""false""#, "0", "{}", "[]"] {
        let changed = result.replacen(
            r#""complete":false"#,
            &format!(r#""complete":{invalid}"#),
            1,
        );
        assert_eq!(
            accept(&corrupt(&report, &result, &changed).unwrap(), &expectations),
            Err(AcceptanceDefect::Shape),
            "{invalid}"
        );
    }
}

#[test]
fn available_and_unavailable_candidates_without_an_expected_commit_remain_supported() {
    use amiss_wire::requests::{
        IndexIdentityScope, IndexSnapshotIdentity, IndexSnapshotKind, IndexSnapshotSchema,
    };

    let (wire, mut expectations) = accepted_report();
    expectations.candidate_commit = None;
    let mut report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    assert_eq!(accept(&wire, &expectations), Ok(0));
    let model::Evaluation::Resolved(mut evaluation) = report.payload.evaluation.clone() else {
        panic!("the fixture has a resolved evaluation");
    };
    let model::Snapshot::Available(CandidateSnapshot::Git(candidate)) = &evaluation.candidate
    else {
        panic!("the fixture has a Git candidate");
    };
    let index = IndexSnapshotIdentity {
        base_commit_oid: expectations.base_commit.clone(),
        base_object_format: candidate.object_format,
        entry_count: 0,
        identity_scope: IndexIdentityScope::CompleteLogicalIndex,
        index_projection_digest: hb("test", b"index projection"),
        kind: IndexSnapshotKind::Index,
        snapshot_digest: hb("test", b"snapshot"),
        snapshot_schema: IndexSnapshotSchema::Current,
    };
    evaluation.candidate = model::Snapshot::Available(CandidateSnapshot::Index(index));
    report.payload.evaluation = model::Evaluation::Resolved(evaluation.clone());
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(0));
    report.payload.result.complete = false;
    report.payload.result.exit_code = 2;
    report.payload.result.status = model::ReportStatus::Incomplete;
    evaluation.candidate = model::Snapshot::Unavailable(model::UnavailableSnapshot {
        kind: model::UnavailableSnapshotKind::Unavailable,
        reasons: vec![model::SnapshotUnavailableReason::NotSupplied],
        request_digest: None,
    });
    report.payload.evaluation = model::Evaluation::Resolved(evaluation);
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(2));
    report.payload.evaluation = model::Evaluation::Unavailable(model::UnavailableEvaluation {
        reasons: Vec::new(),
        request_digest: None,
        status: model::UnavailableStatus::Unavailable,
    });
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(2));
}

#[test]
fn candidates_without_an_expected_commit_still_require_a_snapshot_shape() {
    let (wire, mut expectations) = accepted_report();
    expectations.candidate_commit = None;
    let report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let model::Snapshot::Available(CandidateSnapshot::Git(candidate)) = &evaluation.candidate
    else {
        panic!("the fixture has a Git candidate");
    };
    let positional = serde_json_canonicalizer::to_string(&(
        &candidate.commit_oid,
        candidate.kind,
        candidate.object_format,
        &candidate.tree_oid,
    ))
    .unwrap();
    let commit = serde_json::to_string(&candidate.commit_oid).unwrap();
    let candidate = serde_json_canonicalizer::to_string(candidate).unwrap();
    assert_eq!(candidate.matches(&commit).count(), 1);
    let malformed_git = candidate.replacen(&commit, r#""not-an-oid""#, 1);
    for invalid in [
        "null",
        "true",
        "7",
        r#""index""#,
        "[]",
        "{}",
        &positional,
        &malformed_git,
        r#"{"kind":"index"}"#,
        r#"{"kind":"unavailable"}"#,
    ] {
        assert_eq!(
            accept(
                &corrupt(&report, &candidate, invalid).unwrap(),
                &expectations
            ),
            Err(AcceptanceDefect::Shape),
            "{invalid}"
        );
    }
    let evaluation = serde_json_canonicalizer::to_string(evaluation).unwrap();
    let changed = evaluation.replacen(&format!(r#""candidate":{candidate},"#), "", 1);
    assert_eq!(
        accept(
            &corrupt(&report, &evaluation, &changed).unwrap(),
            &expectations
        ),
        Err(AcceptanceDefect::Shape)
    );
}

#[test]
fn metadata_extensions_are_rejected_before_the_payload_digest_check() {
    let (wire, expectations) = accepted_report();
    let report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let payload = &report.payload;
    let adapter = payload.engine.adapters.first().unwrap();
    let source = std::str::from_utf8(&wire).unwrap();
    for object in [
        serde_json_canonicalizer::to_string(payload).unwrap(),
        serde_json_canonicalizer::to_string(&payload.engine).unwrap(),
        serde_json_canonicalizer::to_string(&payload.engine.action_provenance).unwrap(),
        serde_json_canonicalizer::to_string(adapter).unwrap(),
        serde_json_canonicalizer::to_string(&adapter.contract_descriptor).unwrap(),
        serde_json_canonicalizer::to_string(&payload.result).unwrap(),
        serde_json_canonicalizer::to_string(&payload.summary).unwrap(),
        serde_json_canonicalizer::to_string(&payload.summary.documents).unwrap(),
        serde_json_canonicalizer::to_string(&payload.summary.findings).unwrap(),
        serde_json_canonicalizer::to_string(&payload.summary.references).unwrap(),
    ] {
        let extended = object.replacen(
            '{',
            r#"{"future":{"\ud83d\ude00":[null,true,-7],"\ue000":"extra"},"#,
            1,
        );
        assert_eq!(source.matches(&object).count(), 1);
        let mut stale =
            amiss_fixtures::canonical_json(source.replacen(&object, &extended, 1).as_bytes())
                .unwrap();
        stale.push(b'\n');
        assert_eq!(
            accept(&stale, &expectations),
            Err(AcceptanceDefect::Shape),
            "{object}"
        );
        assert_eq!(
            accept(
                &corrupt(&report, &object, &extended).unwrap(),
                &expectations
            ),
            Err(AcceptanceDefect::Shape),
            "{object}"
        );
    }
}

#[test]
fn the_core_reader_keeps_strict_json_and_exact_canonical_bytes() {
    let (wire, expectations) = accepted_report();
    let original = String::from_utf8(wire).unwrap();
    for inserted in [
        r#""future":-0,"#,
        r#""future":1.0,"#,
        r#""future":1e0,"#,
        r#""future":9007199254740992,"#,
        r#""future":0,"future":0,"#,
        r#""future":0,"\u0066uture":0,"#,
    ] {
        let bytes = original.replacen(r#""payload":{"#, &format!(r#""payload":{{{inserted}"#), 1);
        assert_eq!(
            accept(bytes.as_bytes(), &expectations),
            Err(AcceptanceDefect::Shape),
            "{inserted}"
        );
    }
    for changed in [
        format!(" {original}"),
        format!("{original}\n"),
        original.replace("sha256:", "sha256\\u003a"),
    ] {
        assert_eq!(
            accept(changed.as_bytes(), &expectations),
            Err(AcceptanceDefect::Noncanonical)
        );
    }
}

fn bind(report: &mut model::ReportEnvelope) -> Vec<u8> {
    report.payload_digest = hj_serde(PAYLOAD_SCHEMA, |mut writer| {
        serde_json_canonicalizer::to_writer(&report.payload, &mut writer)
    })
    .unwrap();
    let mut bytes = serde_json_canonicalizer::to_vec(report).unwrap();
    bytes.push(b'\n');
    bytes
}

#[test]
fn wire_corruption_preserves_canonicality_and_rebinds_the_payload() {
    let (wire, expectations) = accepted_report();
    let report: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
    let original = serde_json_canonicalizer::to_string(&report.payload.result).unwrap();
    let mut result = report.payload.result.clone();
    result.finding_count += 1;
    let mut wires = Vec::new();
    for replacement in [
        serde_json_canonicalizer::to_string(&result).unwrap(),
        serde_json::to_string_pretty(&result).unwrap(),
    ] {
        let wire = corrupt(&report, &original, &replacement).unwrap();
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::FindingCount)
        );
        let decoded: model::ReportEnvelope = serde_json::from_slice(&wire).unwrap();
        assert_eq!(
            amiss_fixtures::canonical_json(&wire).unwrap(),
            serde_json_canonicalizer::to_vec(&decoded).unwrap()
        );
        assert_eq!(
            decoded.payload_digest,
            hb(
                PAYLOAD_SCHEMA,
                &serde_json_canonicalizer::to_vec(&decoded.payload).unwrap()
            )
        );
        wires.push(wire);
    }
    assert_eq!(wires.first(), wires.last());
    let payload = serde_json_canonicalizer::to_string(&report.payload).unwrap();
    assert!(corrupt(&report, &payload, "{}{}").is_err());
}
