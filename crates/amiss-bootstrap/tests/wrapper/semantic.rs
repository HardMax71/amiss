use amiss_bootstrap::result::BootstrapResult;
use amiss_wire::{
    assessment::Nullable,
    report::{
        PAYLOAD_SCHEMA,
        model::{Controls, ReportEnvelope, SemanticEvidenceProducer, SemanticEvidenceProvenance},
    },
    requests::{ControlsRequest, SuppliedSemanticEvidence},
    semantic::{
        self, SemanticEvidenceEnvelope,
        observation::{Observation, SiteBuildObservation},
    },
};
use sha2::Digest as _;

use super::{Release, invoke, plant, sealed_run, settled, stderr_names};

pub(super) fn capture(staged: &Release) {
    let mut run = sealed_run(staged);
    let document = semantic::parse(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();
    let producer = &document.payload.producer;
    let mut report: ReportEnvelope = serde_json::from_slice(&run.wire).unwrap();
    let Controls::Resolved(controls) = &mut report.payload.controls else {
        panic!("the report has resolved controls");
    };
    controls.semantic_evidence = Some(vec![SemanticEvidenceProvenance {
        payload_digest: document.payload_digest,
        producer: SemanticEvidenceProducer {
            identity: producer.identity.clone(),
            input_digest: producer.input_digest,
            kind: producer.kind,
            version: producer.version.clone(),
        },
    }]);
    report.payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&report.payload).unwrap())
            .finalize()
            .0,
    );
    run.wire = serde_json_canonicalizer::to_vec(&report).unwrap();
    run.wire.push(b'\n');
    run.requests.controls.semantic_evidence = vec![SuppliedSemanticEvidence {
        expected_context_digest: producer.context_digest,
        value: document,
    }];
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

    let mut cases = malformed_controls(&run.requests.controls)
        .into_iter()
        .map(|bytes| ("controls-request-invalid", bytes))
        .collect::<Vec<_>>();
    let supplied = run.requests.controls.semantic_evidence.first().unwrap();
    for invalid in semantic_defects(&supplied.value) {
        let mut controls = run.requests.controls.clone();
        controls.semantic_evidence.first_mut().unwrap().value = invalid;
        cases.push((
            "semantic-evidence-invalid",
            serde_json_canonicalizer::to_vec(&controls).unwrap(),
        ));
    }
    for (diagnostic, bytes) in cases {
        run.controls_input = Some(bytes);
        let invocation = invoke(staged, &run, "result", false);
        assert_eq!(invocation.output.status.code(), Some(2));
        assert_eq!(settled(&invocation), Some(BootstrapResult::TamperedRuntime));
        assert!(std::fs::read(&invocation.report).unwrap().is_empty());
        stderr_names(&invocation, diagnostic, "invalid semantic input");
    }
}

fn malformed_controls(request: &ControlsRequest) -> Vec<Vec<u8>> {
    let document = &request.semantic_evidence.first().unwrap().value;
    let envelope = String::from_utf8(serde_json_canonicalizer::to_vec(document).unwrap()).unwrap();
    let payload =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap()).unwrap();
    let controls = String::from_utf8(serde_json_canonicalizer::to_vec(&request).unwrap()).unwrap();
    let producer = serde_json_canonicalizer::to_string(&document.payload.producer).unwrap();
    let subject = serde_json_canonicalizer::to_string(&document.payload.subject).unwrap();
    let cases = [
        (
            envelope.as_bytes().to_vec(),
            envelope.replacen('{', "{\"future\":true,", 1).into_bytes(),
        ),
        (
            payload.as_bytes().to_vec(),
            payload.replacen('{', "{\"future\":true,", 1).into_bytes(),
        ),
        (
            serde_json_canonicalizer::to_vec(&document.payload.producer).unwrap(),
            producer.replacen('{', "{\"future\":true,", 1).into_bytes(),
        ),
        (
            serde_json_canonicalizer::to_vec(&document.payload.subject).unwrap(),
            subject.replacen('{', "{\"future\":true,", 1).into_bytes(),
        ),
        (
            serde_json_canonicalizer::to_vec(&document.payload.observations).unwrap(),
            br#"[["future-fact"]]"#.to_vec(),
        ),
        (
            serde_json_canonicalizer::to_vec(&document.payload.observations).unwrap(),
            br#"[{"kind":"future-fact"}]"#.to_vec(),
        ),
    ];
    let original_digest = document.payload_digest.to_string();
    cases
        .into_iter()
        .map(|(original, changed)| {
            let original = String::from_utf8(original).unwrap();
            let changed = String::from_utf8(changed).unwrap();
            let changed_payload = payload.replace(&original, &changed);
            let digest = amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(semantic::PAYLOAD_SCHEMA)
                    .chain_update([0_u8])
                    .chain_update(changed_payload.as_bytes())
                    .finalize()
                    .0,
            );
            let changed_envelope = envelope
                .replace(&original, &changed)
                .replace(&original_digest, &digest.to_string());
            assert_ne!(changed_envelope, envelope);
            let malformed = controls.replace(&envelope, &changed_envelope);
            assert_ne!(malformed, controls);
            malformed.into_bytes()
        })
        .collect()
}

fn semantic_defects(
    document: &SemanticEvidenceEnvelope<'static>,
) -> Vec<SemanticEvidenceEnvelope<'static>> {
    let mut wrong_version = document.clone();
    "not a version".clone_into(&mut wrong_version.payload.producer.version);
    let mut duplicate = document.clone();
    duplicate.payload.observations =
        vec![document.payload.observations.first().unwrap().clone(); 2];
    let mut unsorted = document.clone();
    unsorted.payload.observations = ["/z", "/a"]
        .map(|route| {
            Observation::Site(SiteBuildObservation::GeneratedRoute {
                route: route.to_owned(),
                source: Nullable::Null,
                anchors: Vec::new(),
            })
        })
        .map(std::borrow::Cow::Owned)
        .to_vec();
    let mut wrong_context = document.clone();
    wrong_context.payload.producer.context_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("test")
            .chain_update([0_u8])
            .chain_update(b"wrong context")
            .finalize()
            .0,
    );
    let mut stale_digest = document.clone();
    stale_digest.payload.complete = !stale_digest.payload.complete;

    [
        (wrong_version, true),
        (duplicate, true),
        (unsorted, true),
        (wrong_context, true),
        (stale_digest, false),
    ]
    .into_iter()
    .map(|(mut invalid, rebind)| {
        if rebind {
            invalid.payload_digest = amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(semantic::PAYLOAD_SCHEMA)
                    .chain_update([0_u8])
                    .chain_update(serde_json_canonicalizer::to_vec(&invalid.payload).unwrap())
                    .finalize()
                    .0,
            );
        }
        invalid
    })
    .collect()
}
