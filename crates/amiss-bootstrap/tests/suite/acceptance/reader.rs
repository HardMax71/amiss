use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::digest::hb;
use amiss_wire::report::PAYLOAD_SCHEMA;
use serde_json::{Value, json};

use super::accepted_report;

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
fn unavailable_evaluations_and_unrequested_candidates_remain_supported() {
    use amiss_wire::report::model::{
        SnapshotUnavailableReason, UnavailableEvaluation, UnavailableSnapshot,
        UnavailableSnapshotKind, UnavailableStatus,
    };

    let (wire, mut expectations) = accepted_report();
    expectations.candidate_commit = None;
    let mut report: Value = serde_json::from_slice(&wire).unwrap();
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
fn additive_core_fields_are_accepted_only_with_a_matching_payload_digest() {
    let (wire, expectations) = accepted_report();
    let original: Value = serde_json::from_slice(&wire).unwrap();
    for path in [
        "/payload",
        "/payload/engine",
        "/payload/evaluation/base",
        "/payload/result",
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
        assert_eq!(accept(&bind(&mut report), &expectations), Ok(0), "{path}");
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
