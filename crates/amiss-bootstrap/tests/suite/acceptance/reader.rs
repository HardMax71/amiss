use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::digest::hb;
use amiss_wire::report::PAYLOAD_SCHEMA;
use serde_json::{Value, json};

use super::accepted_report;

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
                report.payload_digest = hb(
                    PAYLOAD_SCHEMA,
                    &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
                );
                let mut bytes = serde_json_canonicalizer::to_vec(&report).unwrap();
                bytes.push(b'\n');
                let (normal, sealed) = if valid.contains(&(complete, status, exit_code)) {
                    (Ok(exit_code), Ok(i64::from(exit_code)))
                } else {
                    (
                        Err(ReportDefect::InvalidResult),
                        Err(AcceptanceDefect::Completeness),
                    )
                };
                assert_eq!(
                    validate_envelope(&bytes).map(|(_, _, verdict)| verdict.code()),
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
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
    let result =
        String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload.result).unwrap())
            .unwrap();
    let error_count = format!("\"error_count\":{}", report.payload.result.error_count);
    let finding_count = format!("\"finding_count\":{}", report.payload.result.finding_count);
    let omitted_error_count = format!("{error_count},");
    let omitted_finding_count = format!("{finding_count},");
    let wire = String::from_utf8(wire).unwrap();
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
        let altered_payload = payload.replace(&result, &invalid);
        let altered = wire.replace(&payload, &altered_payload).replace(
            &report.payload_digest.to_string(),
            &hb(PAYLOAD_SCHEMA, altered_payload.as_bytes()).to_string(),
        );
        assert_eq!(
            validate_envelope(altered.as_bytes()).map(drop),
            Err(ReportDefect::InvalidResult),
            "{invalid}"
        );
        assert_eq!(
            accept(altered.as_bytes(), &expectations),
            Err(AcceptanceDefect::Shape),
            "{invalid}"
        );
    }
}

#[test]
fn core_defects_keep_their_order_when_later_fields_are_also_wrong() {
    let (wire, expectations) = accepted_report();
    let original: Value = serde_json::from_slice(&wire).unwrap();
    let cases = [
        (
            "/payload/engine/engine_digest",
            json!(format!("sha256:{}", "0".repeat(64))),
            AcceptanceDefect::Engine,
        ),
        (
            "/payload/evaluation/base/commit_oid",
            json!("a".repeat(40)),
            AcceptanceDefect::BaseIdentity,
        ),
        (
            "/payload/evaluation/candidate/kind",
            json!("git-tag"),
            AcceptanceDefect::CandidateIdentity,
        ),
        (
            "/payload/result/complete",
            json!(false),
            AcceptanceDefect::Completeness,
        ),
        (
            "/payload/result/finding_count",
            json!(1),
            AcceptanceDefect::FindingCount,
        ),
    ];
    for (first, (path, _, expected)) in cases.iter().enumerate() {
        let mut report = original.clone();
        for (path, value, _) in cases.iter().skip(first) {
            *report.pointer_mut(path).unwrap() = value.clone();
        }
        assert_eq!(
            accept(&bind(&mut report), &expectations),
            Err(*expected),
            "{path}"
        );
    }
    let mut report = original;
    report["payload"]["engine"]["engine_digest"] = json!(format!("sha256:{}", "0".repeat(64)));
    report["payload"]["result"] = Value::Null;
    assert_eq!(
        accept(&bind(&mut report), &expectations),
        Err(AcceptanceDefect::Engine)
    );
    report["payload_digest"] = json!(format!("sha256:{}", "0".repeat(64)));
    let mut wire = serde_json_canonicalizer::to_vec(&report).unwrap();
    wire.push(b'\n');
    assert_eq!(
        accept(&wire, &expectations),
        Err(AcceptanceDefect::PayloadDigest)
    );
}

#[test]
fn core_objects_cannot_be_replaced_by_positional_arrays() {
    let (wire, expectations) = accepted_report();
    let original: Value = serde_json::from_slice(&wire).unwrap();
    for (path, expected) in [
        ("", AcceptanceDefect::Shape),
        ("/payload", AcceptanceDefect::Shape),
        ("/payload/engine", AcceptanceDefect::Engine),
        ("/payload/evaluation", AcceptanceDefect::Shape),
        ("/payload/evaluation/base", AcceptanceDefect::BaseIdentity),
        (
            "/payload/evaluation/candidate",
            AcceptanceDefect::CandidateIdentity,
        ),
        ("/payload/result", AcceptanceDefect::Shape),
    ] {
        let mut report = original.clone();
        let value = report.pointer_mut(path).unwrap();
        *value = Value::Array(value.as_object().unwrap().values().cloned().collect());
        let wire = if path.is_empty() {
            let mut bytes = serde_json_canonicalizer::to_vec(&report).unwrap();
            bytes.push(b'\n');
            bytes
        } else {
            bind(&mut report)
        };
        assert_eq!(accept(&wire, &expectations), Err(expected), "{path}");
    }
}

#[test]
fn core_status_tags_are_strings_and_completion_is_boolean() {
    let (wire, expectations) = accepted_report();
    let original: Value = serde_json::from_slice(&wire).unwrap();
    for (path, value, expected) in [
        (
            "/payload/evaluation/status",
            json!({"unavailable": null}),
            AcceptanceDefect::Shape,
        ),
        (
            "/payload/evaluation/candidate/kind",
            json!({"git-commit": null}),
            AcceptanceDefect::CandidateIdentity,
        ),
    ] {
        let mut report = original.clone();
        let (parent, key) = path.rsplit_once('/').unwrap();
        report.pointer_mut(parent).unwrap()[key] = value;
        assert_eq!(
            accept(&bind(&mut report), &expectations),
            Err(expected),
            "{path}"
        );
    }
    let mut report = original;
    report["payload"]["result"]["exit_code"] = json!(2);
    report["payload"]["result"]["status"] = json!("incomplete");
    report["payload"]["result"]["complete"] = json!(false);
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(2));
    for invalid in [Value::Null, json!("false"), json!(0), json!({}), json!([])] {
        report["payload"]["result"]["complete"] = invalid;
        assert_eq!(
            accept(&bind(&mut report), &expectations),
            Err(AcceptanceDefect::Shape)
        );
    }
}

#[test]
fn available_and_unavailable_candidates_without_an_expected_commit_remain_supported() {
    use amiss_wire::report::model::{
        SnapshotUnavailableReason, UnavailableEvaluation, UnavailableSnapshot,
        UnavailableSnapshotKind, UnavailableStatus,
    };
    use amiss_wire::requests::{
        IndexIdentityScope, IndexSnapshotIdentity, IndexSnapshotKind, IndexSnapshotSchema,
    };

    let (wire, mut expectations) = accepted_report();
    expectations.candidate_commit = None;
    let mut report: Value = serde_json::from_slice(&wire).unwrap();
    assert_eq!(accept(&wire, &expectations), Ok(0));
    let candidate = &report["payload"]["evaluation"]["candidate"];
    let index = IndexSnapshotIdentity {
        base_commit_oid: expectations.base_commit.clone(),
        base_object_format: serde_json::from_value(candidate["object_format"].clone()).unwrap(),
        entry_count: 0,
        identity_scope: IndexIdentityScope::CompleteLogicalIndex,
        index_projection_digest: hb("test", b"index projection"),
        kind: IndexSnapshotKind::Index,
        snapshot_digest: hb("test", b"snapshot"),
        snapshot_schema: IndexSnapshotSchema::Current,
    };
    report["payload"]["evaluation"]["candidate"] = serde_json::to_value(index).unwrap();
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(0));
    report["payload"]["result"]["complete"] = json!(false);
    report["payload"]["result"]["exit_code"] = json!(2);
    report["payload"]["result"]["status"] = json!("incomplete");
    report["payload"]["evaluation"]["candidate"] = serde_json::to_value(UnavailableSnapshot {
        kind: UnavailableSnapshotKind::Unavailable,
        reasons: vec![SnapshotUnavailableReason::NotSupplied],
        request_digest: None,
    })
    .unwrap();
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(2));
    report["payload"]["evaluation"] = serde_json::to_value(UnavailableEvaluation {
        reasons: Vec::new(),
        request_digest: None,
        status: UnavailableStatus::Unavailable,
    })
    .unwrap();
    assert_eq!(accept(&bind(&mut report), &expectations), Ok(2));
}

#[test]
fn candidates_without_an_expected_commit_still_require_a_snapshot_shape() {
    let (wire, mut expectations) = accepted_report();
    expectations.candidate_commit = None;
    let mut report: Value = serde_json::from_slice(&wire).unwrap();
    let candidate = &report["payload"]["evaluation"]["candidate"];
    let positional = json!([
        candidate["commit_oid"],
        candidate["kind"],
        candidate["object_format"],
        candidate["tree_oid"],
    ]);
    let mut malformed_git = candidate.clone();
    malformed_git["commit_oid"] = json!("not-an-oid");
    for invalid in [
        Value::Null,
        json!(true),
        json!(7),
        json!("index"),
        json!([]),
        json!({}),
        positional,
        malformed_git,
        json!({"kind": "index"}),
        json!({"kind": "unavailable"}),
    ] {
        report["payload"]["evaluation"]["candidate"] = invalid;
        assert_eq!(
            accept(&bind(&mut report), &expectations),
            Err(AcceptanceDefect::Shape),
            "{}",
            report["payload"]["evaluation"]["candidate"]
        );
    }
    report["payload"]["evaluation"]
        .as_object_mut()
        .unwrap()
        .remove("candidate");
    assert_eq!(
        accept(&bind(&mut report), &expectations),
        Err(AcceptanceDefect::Shape)
    );
}

#[test]
fn result_extensions_are_rejected_after_the_payload_digest_check() {
    let (wire, expectations) = accepted_report();
    let original: Value = serde_json::from_slice(&wire).unwrap();
    for (path, expected) in [
        ("/payload", Ok(0)),
        ("/payload/engine", Ok(0)),
        ("/payload/result", Err(AcceptanceDefect::Shape)),
    ] {
        let mut report = original.clone();
        report.pointer_mut(path).unwrap()["future"] =
            json!({"\u{1f600}": [null, true, -7], "\u{e000}": "extra"});
        let mut stale = serde_json_canonicalizer::to_vec(&report).unwrap();
        stale.push(b'\n');
        assert_eq!(
            accept(&stale, &expectations),
            Err(AcceptanceDefect::PayloadDigest),
            "{path}"
        );
        assert_eq!(
            accept(&bind(&mut report), &expectations),
            expected,
            "{path}"
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

fn bind(report: &mut Value) -> Vec<u8> {
    let payload = serde_json_canonicalizer::to_vec(&report["payload"]).unwrap();
    report["payload_digest"] = json!(hb(PAYLOAD_SCHEMA, &payload));
    let mut bytes = serde_json_canonicalizer::to_vec(report).unwrap();
    bytes.push(b'\n');
    bytes
}
