use amiss_wire::{
    de::ErrorKind,
    digest::{hb, hj},
    external::{self, AssessmentDefect},
    json,
};
use serde_json::{Value, json};

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-plan.json");
const ASSESSMENT: &[u8] =
    include_bytes!("../../../../spec/examples/scanner-external-assessment.json");

#[test]
fn external_envelopes_keep_strict_inputs_and_complete_payload_digests() {
    let readers: [fn(&[u8]) -> bool; 2] = [
        |bytes| external::parse_plan(bytes).is_ok(),
        |bytes| external::parse_assessment(bytes).is_ok(),
    ];
    for ((bytes, domain), read) in [
        (PLAN, external::PLAN_PAYLOAD_SCHEMA),
        (ASSESSMENT, external::ASSESSMENT_PAYLOAD_SCHEMA),
    ]
    .into_iter()
    .zip(readers)
    {
        let original: Value = serde_json::from_slice(bytes).unwrap();
        assert!(read(bytes));
        for path in ["/payload", "/payload/engine"] {
            let mut extended = original.clone();
            extended.pointer_mut(path).unwrap()["future"] = json!({"😀": 1, "\u{e000}": 2});
            let payload = serde_json::to_vec(&extended["payload"]).unwrap();
            extended["payload_digest"] = json!(hj(domain, &json::parse(&payload).unwrap()));
            assert!(read(&serde_json::to_vec(&extended).unwrap()));
            extended.pointer_mut(path).unwrap()["future"] = json!({"😀": 2, "\u{e000}": 1});
            assert!(!read(&serde_json::to_vec(&extended).unwrap()));
        }

        let mut extended = original.clone();
        let mut nested = Value::Null;
        for _ in 0..510 {
            nested = json!([nested]);
        }
        extended["payload"]["future"] = nested;
        extended["payload_digest"] = json!(hb(
            domain,
            &serde_json_canonicalizer::to_vec(&extended["payload"]).unwrap()
        ));
        let bytes = serde_json::to_vec(&extended).unwrap();
        assert!(read(&bytes));
        let nested = extended["payload"]["future"].take();
        extended["payload"]["future"] = json!([nested]);
        assert!(!read(&serde_json::to_vec(&extended).unwrap()));

        let text = String::from_utf8(serde_json::to_vec(&original).unwrap()).unwrap();
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
        let positional = json!([
            original["schema"],
            original["payload"],
            original["payload_digest"]
        ]);
        assert!(!read(&serde_json::to_vec(&positional).unwrap()));
        let oversized = vec![b' '; usize::try_from(external::EXTERNAL_DOCUMENT_BYTES + 1).unwrap()];
        assert!(!read(&oversized));
    }
}

#[test]
fn external_payloads_keep_structural_paths_and_semantic_validation_order() {
    let mut plan: Value = serde_json::from_slice(PLAN).unwrap();
    plan["payload"]["engine"]["engine_version"] = json!(1);
    let defect = external::parse_plan(&serde_json::to_vec(&plan).unwrap()).unwrap_err();
    assert_eq!(defect.path, "$.payload.engine.engine_version");
    assert_eq!(defect.kind, ErrorKind::WrongType);
    plan["payload"]["engine"]["engine_version"] = json!("");
    assert_eq!(
        external::parse_plan(&serde_json::to_vec(&plan).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::DigestMismatch
    );
    plan["payload_digest"] = json!(hb(
        external::PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&plan["payload"]).unwrap()
    ));
    assert_eq!(
        external::parse_plan(&serde_json::to_vec(&plan).unwrap())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );

    let mut assessment: Value = serde_json::from_slice(ASSESSMENT).unwrap();
    assessment["payload"]["producer"]["version"] = json!(null);
    let Err(AssessmentDefect::Wire(defect)) =
        external::parse_assessment(&serde_json::to_vec(&assessment).unwrap())
    else {
        panic!("the producer version must be a string");
    };
    assert_eq!(defect.path, "$.payload.producer.version");
    assert_eq!(defect.kind, ErrorKind::WrongType);
    assessment["payload"]["producer"]["version"] = json!("");
    assert!(matches!(
        external::parse_assessment(&serde_json::to_vec(&assessment).unwrap()),
        Err(AssessmentDefect::Contract(_))
    ));
}
