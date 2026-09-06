use amiss_wire::{
    de::ErrorKind,
    digest::hb,
    external::{self, EvidenceDefect},
    json,
};
use serde_json::{Value, json};

const EVIDENCE: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-evidence.json");

#[test]
fn assessments_use_the_digest_of_all_evidence_fields() {
    let original: Value = serde_json::from_slice(EVIDENCE).unwrap();
    let (expected, original_digest) = external::parse_evidence(EVIDENCE).unwrap();
    for path in ["", "/producer", "/rows/0"] {
        let mut extended = original.clone();
        extended.pointer_mut(path).unwrap()["future"] = json!({"😀": "\t", "\u{e000}": null});
        let bytes = serde_json::to_vec_pretty(&extended).unwrap();
        let (typed, digest) = external::parse_evidence(&bytes).unwrap();
        assert_eq!(typed, expected);
        assert_ne!(digest, original_digest);
        assert_eq!(
            digest,
            hb(
                external::EVIDENCE_SCHEMA,
                &serde_json_canonicalizer::to_vec(&json::parse(&bytes).unwrap()).unwrap()
            )
        );
        assert_eq!(
            external::parse_evidence(&serde_json::to_vec(&extended).unwrap())
                .unwrap()
                .1,
            digest
        );
        let assessment = external::assess(
            include_bytes!("../../../../spec/examples/scanner-external-plan.json"),
            &bytes,
            "0.0.0",
            hb("test", b"engine"),
        )
        .unwrap();
        assert_eq!(
            external::parse_assessment(&assessment)
                .unwrap()
                .payload
                .subject
                .evidence_digest,
            digest
        );
        extended.pointer_mut(path).unwrap()["future"] = json!(false);
        let (changed, changed_digest) =
            external::parse_evidence(&serde_json::to_vec(&extended).unwrap()).unwrap();
        assert_eq!(changed, typed);
        assert_ne!(changed_digest, digest);
    }
}

#[test]
fn evidence_capture_keeps_strict_bounds_and_requires_an_object() {
    let mut evidence: Value = serde_json::from_slice(EVIDENCE).unwrap();
    let mut nested = Value::Null;
    for _ in 0..511 {
        nested = json!([nested]);
    }
    evidence["future"] = nested;
    let bytes = serde_json::to_vec(&evidence).unwrap();
    serde_json::from_slice::<external::ExternalEvidence>(&bytes).unwrap();
    external::parse_evidence(&bytes).unwrap();
    let nested = evidence["future"].take();
    evidence["future"] = json!([nested]);
    assert!(matches!(
        external::parse_evidence(&serde_json::to_vec(&evidence).unwrap()),
        Err(EvidenceDefect::Wire(amiss_wire::de::Error {
            kind: ErrorKind::Json(json::Error {
                kind: json::ErrorKind::DepthLimit,
                ..
            }),
            ..
        }))
    ));
    for invalid in [
        br#"{"future":0,"\u0066uture":1}"#.as_slice(),
        br#"{"future":-0}"#,
        br#"{"future":0.5}"#,
        br#"{"future":1e0}"#,
        br#"{"future":9007199254740992}"#,
        b"{} {}",
        b"\xff",
    ] {
        assert!(matches!(
            external::parse_evidence(invalid),
            Err(EvidenceDefect::Wire(amiss_wire::de::Error {
                kind: ErrorKind::Json(_),
                ..
            }))
        ));
    }
    let positional = json!([
        evidence["schema"],
        evidence["plan_payload_digest"],
        evidence["producer"],
        evidence["rows"]
    ]);
    let Err(EvidenceDefect::Wire(defect)) =
        external::parse_evidence(&serde_json::to_vec(&positional).unwrap())
    else {
        panic!("the evidence root must be an object");
    };
    assert_eq!(
        (defect.path.as_str(), defect.kind),
        ("$", ErrorKind::WrongType)
    );
    let oversized = vec![b' '; usize::try_from(external::EXTERNAL_DOCUMENT_BYTES + 1).unwrap()];
    assert!(matches!(
        external::parse_evidence(&oversized),
        Err(EvidenceDefect::Wire(amiss_wire::de::Error {
            kind: ErrorKind::LimitExceeded,
            ..
        }))
    ));
}
