use std::borrow::Cow;

use amiss_controller::bind_semantic_evidence;
use amiss_wire::{
    assessment::Nullable,
    digest::{hb, sha256},
    semantic::{
        SemanticEvidenceTemplate, SemanticProducer, TemplateSchema, observation::Observation,
        record,
    },
};

#[test]
fn controller_binding_preserves_candidate_context_and_typed_observations() {
    let observation = Observation::Record(record::Observation {
        kind: record::ObservationKind::Current,
        name: "rust/api".parse().unwrap(),
        records: vec![record::Record {
            key: "é".to_owned(),
            value: "quote\" and newline\n".to_owned(),
        }],
    });
    let template = SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: SemanticProducer {
            kind: amiss_wire::semantic::SemanticProducerKind::RecordSet,
            identity: "fixture".parse().unwrap(),
            version: "1".to_owned(),
            context_digest: hb("test", b"context"),
            input_digest: hb("test", b"input"),
        },
        complete: true,
        observations: vec![Cow::Borrowed(&observation)].into(),
    };
    let expected_producer = template.producer.clone();
    let expected_observation = observation.clone();
    let candidates = [
        hb("test", b"first candidate"),
        hb("test", b"second candidate"),
    ];
    let bindings = candidates.map(|candidate| {
        bind_semantic_evidence(std::slice::from_ref(&template), &[], &[], candidate).unwrap()
    });
    drop(template);
    drop(observation);
    let mut previous = None;
    for (candidate, bound) in candidates.into_iter().zip(bindings) {
        let supplied = &bound.supplied[0];
        let document = &supplied.value;
        assert_eq!(
            document.payload.subject.candidate_identity_digest,
            candidate
        );
        assert_eq!(
            document.payload.subject.source_report_payload_digest,
            Nullable::Null
        );
        assert_eq!(document.payload.producer, expected_producer);
        assert_eq!(
            supplied.expected_context_digest,
            expected_producer.context_digest
        );
        assert_eq!(
            document.payload.observations[0].as_ref(),
            &expected_observation
        );
        assert_eq!(document.payload.observations.len(), 1);
        assert!(matches!(document.payload.observations[0], Cow::Owned(_)));
        assert_eq!(amiss_wire::semantic::validate(document), Ok(()));
        assert!(previous.is_none_or(|digest| digest != document.payload_digest));
        previous = Some(document.payload_digest);

        let artifact: serde_json::Value = serde_json::from_slice(&bound.artifact.unwrap()).unwrap();
        let row = &artifact["inputs"][0];
        assert_eq!(row["payload_digest"], document.payload_digest.to_string());
        assert_eq!(
            row["envelope_digest"],
            sha256(&serde_json_canonicalizer::to_vec(document).unwrap()).to_string()
        );
    }
}
