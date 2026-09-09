use amiss_bootstrap::supervise::{AcceptanceDefect, Expectations, accept};
use amiss_wire::{
    digest::hb,
    report::{
        PAYLOAD_SCHEMA, emit_report,
        model::{Controls, ReportEnvelope, SemanticEvidenceProducer, SemanticEvidenceProvenance},
    },
    semantic::SemanticProducerKind,
};

use super::{FLOOR_DIGEST, FOREIGN_DIGEST, GOLDEN};

fn semantic_report() -> (ReportEnvelope, Expectations) {
    let (mut report, mut expectations) = GOLDEN.clone();
    let Controls::Resolved(controls) = &mut report.payload.controls else {
        panic!("resolved controls");
    };
    let expected = SemanticEvidenceProvenance {
        payload_digest: FLOOR_DIGEST.parse().unwrap(),
        producer: SemanticEvidenceProducer {
            identity: "producer".parse().unwrap(),
            input_digest: FLOOR_DIGEST.parse().unwrap(),
            kind: SemanticProducerKind::RecordSet,
            version: "1".to_owned(),
        },
    };
    controls.semantic_evidence = Some(vec![expected.clone()]);
    expectations.sealed.as_mut().unwrap().semantic_evidence = vec![expected];
    (report, expectations)
}

#[test]
fn semantic_evidence_binds_each_producer_fact() {
    let (_, expected) = semantic_report();
    let source = expected.sealed.unwrap().semantic_evidence.pop().unwrap();
    let mut cases: [_; 6] = std::array::from_fn(|_| source.clone());
    cases[1].payload_digest = FOREIGN_DIGEST.parse().unwrap();
    cases[2].producer.input_digest = FOREIGN_DIGEST.parse().unwrap();
    cases[3].producer.identity = "other".parse().unwrap();
    cases[4].producer.kind = SemanticProducerKind::SiteBuild;
    cases[5].producer.version = "2".to_owned();
    for (index, row) in cases.into_iter().enumerate() {
        let (mut report, mut expectations) = semantic_report();
        let Controls::Resolved(controls) = &mut report.payload.controls else {
            panic!("resolved controls");
        };
        controls.semantic_evidence = Some(vec![row]);
        report.payload_digest = hb(
            PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
        );
        let mut wire = Vec::new();
        emit_report(&report, &mut wire).unwrap();
        let expected = (index == 0)
            .then_some(0)
            .ok_or(AcceptanceDefect::SealedControls);
        assert_eq!(accept(&wire, &expectations), expected, "case {index}");
        let sealed = expectations.sealed.as_mut().unwrap();
        sealed.semantic_evidence.clear();
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::SealedControls),
            "unexpected row: case {index}"
        );
    }
}

#[test]
fn semantic_metadata_shape_is_checked_before_bindings() {
    let (mut report, expectations) = semantic_report();
    report.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
    );
    let mut wire = Vec::new();
    emit_report(&report, &mut wire).unwrap();
    let wire = String::from_utf8(wire).unwrap();
    assert_eq!(accept(wire.as_bytes(), &expectations), Ok(0));
    let Controls::Resolved(controls) = &report.payload.controls else {
        panic!("resolved controls");
    };
    let row = controls
        .semantic_evidence
        .as_ref()
        .unwrap()
        .first()
        .unwrap();
    let producer = &row.producer;
    let row_object = String::from_utf8(serde_json_canonicalizer::to_vec(row).unwrap()).unwrap();
    let producer_object =
        String::from_utf8(serde_json_canonicalizer::to_vec(producer).unwrap()).unwrap();
    let cases = [
        (
            row_object.clone(),
            serde_json::to_string(&(row.payload_digest, producer)).unwrap(),
            AcceptanceDefect::Noncanonical,
        ),
        (
            producer_object.clone(),
            serde_json::to_string(&(
                &producer.identity,
                producer.input_digest,
                producer.kind,
                &producer.version,
            ))
            .unwrap(),
            AcceptanceDefect::Shape,
        ),
        (
            serde_json::to_string(&producer.kind).unwrap(),
            serde_json::to_string("unsupported").unwrap(),
            AcceptanceDefect::Shape,
        ),
    ]
    .into_iter()
    .chain([row_object, producer_object].map(|object| {
        let unknown = object.replacen('{', "{\"__unexpected\":true,", 1);
        (object, unknown, AcceptanceDefect::Shape)
    }));
    for (object, invalid, expected) in cases {
        let changed = wire.replace(&object, &invalid);
        assert_ne!(wire, changed);
        assert_eq!(
            accept(changed.as_bytes(), &expectations),
            Err(expected),
            "{invalid}"
        );
    }
}
