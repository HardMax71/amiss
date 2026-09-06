use amiss_bootstrap::result::BootstrapResult;
use amiss_wire::{
    digest::hb,
    report::{
        PAYLOAD_SCHEMA,
        model::{SemanticEvidenceProducer, SemanticEvidenceProvenance},
    },
    requests::SuppliedSemanticEvidence,
};
use serde_json::{Value, json};

use super::{Release, invoke, plant, sealed_run, settled, stderr_names};

pub(super) fn capture(staged: &Release) {
    let mut run = sealed_run(staged);
    let document = amiss_wire::semantic::parse(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();
    let supplied = SuppliedSemanticEvidence {
        value: serde_json::to_value(&document).unwrap(),
        expected_context_digest: document.payload.producer.context_digest,
    };
    run.requests.controls.semantic_evidence = vec![supplied.clone()];
    let producer = document.payload.producer;
    let mut report: Value = serde_json::from_slice(&run.wire).unwrap();
    *report
        .pointer_mut("/payload/controls/semantic_evidence")
        .unwrap() = serde_json::to_value([SemanticEvidenceProvenance {
        payload_digest: document.payload_digest,
        producer: SemanticEvidenceProducer {
            identity: producer.identity,
            input_digest: producer.input_digest,
            kind: producer.kind,
            version: producer.version,
        },
    }])
    .unwrap();
    *report.get_mut("payload_digest").unwrap() = json!(hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(report.get("payload").unwrap()).unwrap()
    ));
    run.wire = serde_json_canonicalizer::to_vec(&report).unwrap();
    run.wire.push(b'\n');
    plant(&run, &run.wire, "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        invocation.output.status.code(),
        Some(0),
        "{:?}",
        invocation.output.stderr
    );
    assert_eq!(settled(&invocation), Some(BootstrapResult::Pass));
    assert_eq!(std::fs::read(&invocation.report).unwrap(), run.wire);

    for (path, value) in [
        ("/future", json!(true)),
        ("/payload/future", json!(true)),
        ("/payload/producer/future", json!(true)),
        ("/payload/subject/future", json!(true)),
        ("/payload/producer/version", json!("not a version")),
        ("/payload/observations", json!([["future-fact"]])),
        (
            "/payload/observations",
            json!([{"kind": "future-fact"}, {"kind": "future-fact"}]),
        ),
        (
            "/payload/observations",
            json!([{"kind": "future-fact", "value": 2}, {"kind": "future-fact", "value": 1}]),
        ),
        (
            "/payload/producer/context_digest",
            json!(hb("test", b"wrong context")),
        ),
        ("/payload_digest", json!(hb("test", b"wrong payload"))),
    ] {
        let mut invalid = supplied.clone();
        let (parent, field) = path.rsplit_once('/').unwrap();
        invalid
            .value
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(field.to_owned(), value);
        if path != "/payload_digest" {
            *invalid.value.get_mut("payload_digest").unwrap() = json!(hb(
                amiss_wire::semantic::PAYLOAD_SCHEMA,
                &serde_json_canonicalizer::to_vec(invalid.value.get("payload").unwrap()).unwrap()
            ));
        }
        run.requests.controls.semantic_evidence = vec![invalid];
        let invocation = invoke(staged, &run, "result", false);
        assert_eq!(invocation.output.status.code(), Some(2), "{path}");
        assert_eq!(settled(&invocation), Some(BootstrapResult::TamperedRuntime));
        assert!(std::fs::read(&invocation.report).unwrap().is_empty());
        stderr_names(&invocation, "semantic-evidence-invalid", path);
    }
}
