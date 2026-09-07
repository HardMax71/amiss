#![cfg(test)]

use amiss_wire::{
    assessment::Nullable,
    digest::hb,
    semantic::{self, SemanticEvidenceEnvelope, observation::SiteBuildObservation},
};
use std::borrow::Cow;

use super::{ErrorKind, Observation, SuppliedSemanticEvidence, validated_envelope};

const EXAMPLE: &[u8] =
    include_bytes!("../../../../../spec/examples/scanner-semantic-evidence.json");
const PATH: &str = "$.semantic_evidence[0]";

#[test]
fn typed_intake_retains_the_original_envelope_allocations() {
    let value = semantic::parse(EXAMPLE).unwrap();
    let observations = value.payload.observations.as_ptr();
    let version = value.payload.producer.version.as_ptr();
    let expected_context_digest = value.payload.producer.context_digest;
    let accepted = validated_envelope(
        SuppliedSemanticEvidence {
            value,
            expected_context_digest,
        },
        PATH,
    )
    .unwrap();
    assert_eq!(accepted.payload.observations.as_ptr(), observations);
    assert_eq!(accepted.payload.producer.version.as_ptr(), version);
}

#[test]
fn typed_intake_rechecks_digest_context_and_semantic_laws() {
    let original = semantic::parse(EXAMPLE).unwrap();
    let expected_context_digest = original.payload.producer.context_digest;
    let mut wrong_version = original.clone();
    "not a version".clone_into(&mut wrong_version.payload.producer.version);
    let mut duplicate = original.clone();
    duplicate.payload.observations = vec![original.payload.observations[0].clone(); 2];
    let mut unsorted = original.clone();
    unsorted.payload.observations = ["/z", "/a"]
        .map(|route| {
            Observation::Site(SiteBuildObservation::GeneratedRoute {
                route: route.to_owned(),
                source: Nullable::Null,
                anchors: Vec::new(),
            })
        })
        .map(Cow::Owned)
        .to_vec();
    let mut too_many = original.clone();
    too_many.payload.observations =
        vec![original.payload.observations[0].clone(); semantic::SEMANTIC_OBSERVATIONS_LIMIT + 1];
    for (mut value, kind) in [
        (wrong_version, ErrorKind::InvalidValue),
        (duplicate, ErrorKind::DuplicateMember),
        (unsorted, ErrorKind::UnsortedSet),
        (too_many, ErrorKind::LimitExceeded),
    ] {
        value.payload_digest = hb(
            semantic::PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&value.payload).unwrap(),
        );
        assert_eq!(
            validated_envelope(
                SuppliedSemanticEvidence {
                    value,
                    expected_context_digest
                },
                PATH
            )
            .map(drop)
            .expect_err("intake refuses the typed semantic defect")
            .kind,
            kind
        );
    }

    let mut stale = original.clone();
    stale.payload.complete = !stale.payload.complete;
    for supplied in [
        SuppliedSemanticEvidence {
            value: stale,
            expected_context_digest,
        },
        SuppliedSemanticEvidence {
            value: original,
            expected_context_digest: hb("test", b"wrong context"),
        },
    ] {
        assert_eq!(
            validated_envelope(supplied, PATH).unwrap_err().kind,
            ErrorKind::DigestMismatch
        );
    }
}

#[test]
fn in_process_intake_keeps_the_exact_encoded_byte_ceiling() {
    let mut document: SemanticEvidenceEnvelope<'static> = serde_json::from_slice(EXAMPLE).unwrap();
    document.payload.observations = vec![Cow::Owned(Observation::Site(
        SiteBuildObservation::GeneratedRoute {
            route: "/é\"\\".to_owned(),
            source: Nullable::Null,
            anchors: vec!["\n\t\u{0008}".to_owned()],
        },
    ))];
    let limit = usize::try_from(semantic::SEMANTIC_EVIDENCE_BYTES).unwrap();
    let initial = serde_json::to_vec(&document).unwrap().len();
    let Observation::Site(SiteBuildObservation::GeneratedRoute { route, .. }) =
        document.payload.observations[0].to_mut()
    else {
        panic!("the fixture is a generated route");
    };
    route.extend(std::iter::repeat_n('a', limit - initial - 1));

    for length in [limit - 1, limit, limit + 1] {
        document.payload_digest = hb(
            semantic::PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
        );
        let encoded = serde_json::to_vec(&document).unwrap();
        assert_eq!(encoded.len(), length);
        let supplied = SuppliedSemanticEvidence {
            value: document.clone(),
            expected_context_digest: document.payload.producer.context_digest,
        };
        let result = validated_envelope(supplied, PATH);
        if length <= limit {
            assert_eq!(result.unwrap(), document);
        } else {
            let defect = result
                .map(drop)
                .expect_err("intake refuses an over-limit envelope");
            assert_eq!(defect.kind, ErrorKind::LimitExceeded);
            assert_eq!(defect.path, "$");
        }
        let Observation::Site(SiteBuildObservation::GeneratedRoute { route, .. }) =
            document.payload.observations[0].to_mut()
        else {
            panic!("the fixture remains a generated route");
        };
        route.push('a');
    }
}
