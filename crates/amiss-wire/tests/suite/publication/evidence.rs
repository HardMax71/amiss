use super::{digest, publication_plan};

use std::{fs, path::Path};

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{hb, hj};
use amiss_wire::json::{self, MAX_SAFE_INTEGER};
use amiss_wire::publication::{
    EVIDENCE_PAYLOAD_SCHEMA, EvidencePayloadSchema, PublicationDeployment, PublicationEvidence,
    PublicationOutcome, PublicationResource, evidence, parse_evidence, parse_plan, plan,
};

pub(super) fn publication_evidence() -> PublicationEvidence {
    let planned = publication_plan();
    let planned_value = plan(&planned).unwrap();
    let planned = parse_plan(&planned_value).unwrap();
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
    let value = evidence(&expected).unwrap();
    let bytes = value;
    let parsed = parse_evidence(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hb(
            EVIDENCE_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&expected).unwrap()
        )
    );

    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let example_bytes = fs::read(examples.join("publication-evidence.json")).unwrap();
    let example = parse_evidence(&example_bytes).unwrap();
    let planned = parse_plan(&fs::read(examples.join("publication-plan.json")).unwrap()).unwrap();
    assert_eq!(example.payload.plan_payload_digest, planned.payload_digest);
    let written = evidence(&example.payload).unwrap();
    assert_eq!(
        written,
        json::canonical(&json::parse(&example_bytes).unwrap())
    );
}

#[test]
fn publication_evidence_refuses_non_success_and_unsafe_attempts() {
    let maximum = u64::try_from(MAX_SAFE_INTEGER).unwrap();
    let mut boundary = publication_evidence();
    boundary.deployment.provider_run_attempt = maximum;
    assert!(evidence(&boundary).is_ok());

    for provider_run_attempt in [0, maximum + 1, u64::MAX] {
        let mut invalid = publication_evidence();
        invalid.deployment.provider_run_attempt = provider_run_attempt;
        let error = evidence(&invalid).unwrap_err();
        assert_eq!(error.path, "$.payload.deployment.provider_run_attempt");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }

    let value = evidence(&publication_evidence()).unwrap();
    let recorded = parse_evidence(&value).unwrap().payload_digest.to_string();
    let failed = String::from_utf8(value)
        .unwrap()
        .replace("\"succeeded\"", "\"failed\"");
    let failed_value = json::parse(failed.as_bytes()).unwrap();
    let rebound = failed.replace(
        &recorded,
        &hj(
            EVIDENCE_PAYLOAD_SCHEMA,
            failed_value.member("payload").unwrap(),
        )
        .to_string(),
    );
    let error = parse_evidence(rebound.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload.deployment.outcome");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn publication_evidence_requires_immutable_deployment_resources() {
    let mut relative_record = publication_evidence();
    relative_record.deployment.record.uri = "deployments/987".to_owned();
    let error = evidence(&relative_record).unwrap_err();
    assert_eq!(error.path, "$.payload.deployment.record.uri");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}
