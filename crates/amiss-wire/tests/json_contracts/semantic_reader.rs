use amiss_wire::assessment::Nullable;
use amiss_wire::semantic::observation::{Observation, SiteBuildObservation};
use amiss_wire::{de::ErrorKind, digest::hb, semantic};

#[test]
fn generated_semantic_digests_keep_the_exact_payload_preimage() {
    let original: semantic::SemanticEvidenceEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();
    for observations in [
        vec![],
        vec![Observation::Site(SiteBuildObservation::GeneratedRoute {
            route: "/é".to_owned(),
            source: Nullable::Null,
            anchors: vec!["quote\" and newline\n".to_owned()],
        })],
        ["z".repeat(131_072), "a".to_owned()]
            .map(|route| {
                Observation::Site(SiteBuildObservation::GeneratedRoute {
                    route,
                    source: Nullable::Null,
                    anchors: Vec::new(),
                })
            })
            .to_vec(),
    ] {
        let (document, bytes) = semantic::envelope(semantic::SemanticEvidence {
            observations,
            ..original.payload.clone()
        })
        .unwrap();
        let preimage = serde_json_canonicalizer::to_vec(&document.payload).unwrap();
        assert_eq!(
            document.payload_digest,
            hb(semantic::PAYLOAD_SCHEMA, &preimage)
        );
        assert_eq!(semantic::validate(&document), Ok(()));
        assert_eq!(semantic::parse(&bytes).unwrap(), document);
    }
}

#[test]
fn decoded_evidence_keeps_the_byte_readers_digest_and_semantic_checks() {
    let bytes = include_bytes!("../../../../spec/examples/scanner-semantic-evidence.json");
    let original: semantic::SemanticEvidenceEnvelope = serde_json::from_slice(bytes).unwrap();
    assert_eq!(semantic::validate(&original), Ok(()));

    let mut tampered = original.clone();
    tampered.payload.complete = !tampered.payload.complete;
    assert_eq!(
        semantic::validate(&tampered).unwrap_err().kind,
        ErrorKind::DigestMismatch
    );

    for (observations, kind) in [
        (
            ["/z", "/a"]
                .map(|route| {
                    Observation::Site(SiteBuildObservation::GeneratedRoute {
                        route: route.to_owned(),
                        source: Nullable::Null,
                        anchors: Vec::new(),
                    })
                })
                .to_vec(),
            ErrorKind::UnsortedSet,
        ),
        (
            vec![original.payload.observations[0].clone(); 2],
            ErrorKind::DuplicateMember,
        ),
        (
            vec![
                original.payload.observations[0].clone();
                semantic::SEMANTIC_OBSERVATIONS_LIMIT + 1
            ],
            ErrorKind::LimitExceeded,
        ),
    ] {
        let mut document = original.clone();
        document.payload.observations = observations;
        document.payload_digest = hb(
            semantic::PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
        );
        let defect = semantic::validate(&document).unwrap_err();
        assert_eq!(defect.kind, kind);
        assert_eq!(
            semantic::parse(&serde_json::to_vec(&document).unwrap()).unwrap_err(),
            defect
        );
    }

    let mut document = original;
    document.payload.producer.version = "not a version".to_owned();
    document.payload_digest = hb(
        semantic::PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
    );
    let defect = semantic::validate(&document).unwrap_err();
    assert_eq!(defect.kind, ErrorKind::InvalidValue);
    assert_eq!(defect.path, "$.payload.producer.version");
    assert_eq!(
        semantic::parse(&serde_json::to_vec(&document).unwrap()).unwrap_err(),
        defect
    );
}
