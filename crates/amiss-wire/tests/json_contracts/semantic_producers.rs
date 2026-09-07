use amiss_wire::{
    de::ErrorKind,
    digest::hb,
    report::model::SemanticEvidenceProducer,
    semantic::{
        self, SemanticEvidenceEnvelope, SemanticEvidenceTemplate, SemanticProducer,
        SemanticProducerKind,
    },
};
use strum::IntoEnumIterator;

#[test]
fn semantic_producer_kinds_are_closed_string_tags_through_provenance() {
    let mut producer = SemanticProducer {
        kind: SemanticProducerKind::SiteBuild,
        identity: "fixture".parse().unwrap(),
        version: "1".to_owned(),
        context_digest: hb("test", b"context"),
        input_digest: hb("test", b"input"),
    };
    for kind in SemanticProducerKind::iter() {
        producer.kind = kind;
        let text = serde_json::to_string(&producer).unwrap();
        assert_eq!(
            serde_json::from_str::<SemanticProducer>(&text).unwrap(),
            producer
        );
        let provenance = SemanticEvidenceProducer {
            kind,
            identity: producer.identity.clone(),
            version: producer.version.clone(),
            input_digest: producer.input_digest,
        };
        let encoded = serde_json::to_string(&provenance).unwrap();
        assert_eq!(
            serde_json::from_str::<SemanticEvidenceProducer>(&encoded).unwrap(),
            provenance
        );
        let tag = serde_json::to_string(&kind).unwrap();
        for invalid in [
            "\"future-producer\"",
            "{\"site-build\":null}",
            "null",
            "12",
            "[]",
        ] {
            let invalid_producer = text.replacen(&tag, invalid, 1);
            let invalid_provenance = encoded.replacen(&tag, invalid, 1);
            assert!(serde_json::from_str::<SemanticProducer>(&invalid_producer).is_err());
            assert!(serde_json::from_str::<SemanticEvidenceProducer>(&invalid_provenance).is_err());
        }
    }
}

#[test]
fn unknown_semantic_producers_fail_even_with_a_matching_payload_digest() {
    let document: SemanticEvidenceEnvelope<'static> = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap()).unwrap();
    let producer =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload.producer).unwrap())
            .unwrap();
    let unknown_producer = producer.replace("\"site-build\"", "\"future-producer\"");
    let unknown = payload.replace(&producer, &unknown_producer);
    assert_ne!(payload, unknown);
    let digest = hb(semantic::PAYLOAD_SCHEMA, unknown.as_bytes());
    let encoded = String::from_utf8(serde_json_canonicalizer::to_vec(&document).unwrap())
        .unwrap()
        .replace(&payload, &unknown)
        .replace(&document.payload_digest.to_string(), &digest.to_string());
    assert_eq!(
        semantic::parse(encoded.as_bytes()).unwrap_err().kind,
        ErrorKind::InvalidValue
    );

    let template: SemanticEvidenceTemplate<'static> = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-semantic-template.json"
    ))
    .unwrap();
    let original = String::from_utf8(serde_json_canonicalizer::to_vec(&template).unwrap()).unwrap();
    let producer =
        String::from_utf8(serde_json_canonicalizer::to_vec(&template.producer).unwrap()).unwrap();
    let unknown_producer = producer.replace("\"record-set\"", "\"future-producer\"");
    let unknown = original.replace(&producer, &unknown_producer);
    assert_ne!(original, unknown);
    assert_eq!(
        semantic::parse_template(unknown.as_bytes())
            .unwrap_err()
            .kind,
        ErrorKind::InvalidValue
    );
}
