use super::{digest, publication_plan};

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::hb;
use amiss_wire::json::MAX_SAFE_INTEGER;
use amiss_wire::publication::{
    EVIDENCE_PAYLOAD_SCHEMA, EvidencePayloadSchema, PUBLICATION_DOCUMENT_BYTES,
    PublicationDeployment, PublicationEvidence, PublicationOutcome, PublicationResource, evidence,
    parse_evidence, plan,
};

pub(super) fn publication_evidence() -> PublicationEvidence {
    let planned = plan(publication_plan()).unwrap();
    PublicationEvidence {
        schema: EvidencePayloadSchema::Current,
        plan_payload_digest: planned.payload_digest,
        producer: planned.payload.producer,
        deployment: PublicationDeployment {
            outcome: PublicationOutcome::Succeeded,
            record: PublicationResource {
                uri: "https://api.github.com/repos/acme/widget/pages/deployments/987".to_owned(),
                digest: digest('8'),
            },
            workflow: PublicationResource {
                uri: "https://github.com/acme/widget/blob/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/.github/workflows/docs.yml".to_owned(),
                digest: digest('9'),
            },
            provider_run_attempt: 2,
        },
        docs: planned.payload.docs,
        target: planned.payload.target,
        site: planned.payload.site,
        product: planned.payload.product,
    }
}

#[test]
fn publication_evidence_round_trips_with_its_plan_and_payload_digests() {
    let expected = publication_evidence();
    let envelope = evidence(expected.clone()).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, PUBLICATION_DOCUMENT_BYTES).unwrap();
    let parsed = parse_evidence(&bytes).unwrap();

    assert_eq!(parsed, envelope);
    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hb(
            EVIDENCE_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&expected).unwrap()
        )
    );
}

#[test]
fn publication_evidence_refuses_non_success_and_unsafe_attempts() {
    let maximum = u64::try_from(MAX_SAFE_INTEGER).unwrap();
    let mut boundary = publication_evidence();
    boundary.deployment.provider_run_attempt = maximum;
    assert!(evidence(boundary).is_ok());

    for provider_run_attempt in [0, maximum + 1, u64::MAX] {
        let mut invalid = publication_evidence();
        invalid.deployment.provider_run_attempt = provider_run_attempt;
        let error = evidence(invalid).unwrap_err();
        assert_eq!(error.path, "$.payload.deployment.provider_run_attempt");
        assert!(matches!(error.kind, ErrorKind::InvalidValue));
    }

    let envelope = evidence(publication_evidence()).unwrap();
    let text = serde_json::to_string(&envelope).unwrap();
    let failed = text.replacen(
        &serde_json::to_string(&envelope.payload.deployment.outcome).unwrap(),
        r#""failed""#,
        1,
    );
    assert_ne!(failed, text);
    let error = parse_evidence(failed.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload.deployment.outcome");
    assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
}

#[test]
fn publication_evidence_requires_immutable_deployment_resources() {
    let mut relative_record = publication_evidence();
    relative_record.deployment.record.uri = "deployments/987".to_owned();
    let error = evidence(relative_record).unwrap_err();
    assert_eq!(error.path, "$.payload.deployment.record.uri");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));
}
