use crate::controls::value::{object, positive_safe_integer, text};
use crate::de::{self, Error, ErrorKind, Obj};
use crate::digest::Digest;
use crate::json::Value;

use super::{
    CompletedSite, DocsCandidate, PUBLICATION_DOCUMENT_BYTES, PublicationProducer,
    PublicationResource, PublicationTarget, decode_facts, decode_resource, docs_value,
    producer_value, resource_value, site_value, target_value,
};

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/publication-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/publication-evidence-payload";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationEvidenceEnvelope {
    pub payload: PublicationEvidence,
    pub payload_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationEvidence {
    pub plan_payload_digest: Digest,
    pub producer: PublicationProducer,
    pub deployment: PublicationDeployment,
    pub docs: DocsCandidate,
    pub target: PublicationTarget,
    pub site: CompletedSite,
    pub product: PublicationResource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicationDeployment {
    pub record: PublicationResource,
    pub workflow: PublicationResource,
    pub provider_run_attempt: u64,
}

/// Parses one closed, digest-bound successful-publication receipt.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// identity or resource, a non-success outcome, an unsafe run attempt, or a
/// payload digest mismatch.
pub fn parse_evidence(bytes: &[u8]) -> Result<PublicationEvidenceEnvelope, Error> {
    let (payload, payload_digest) = crate::bounded_envelope::parse(
        bytes,
        EVIDENCE_ENVELOPE_SCHEMA,
        EVIDENCE_PAYLOAD_SCHEMA,
        PUBLICATION_DOCUMENT_BYTES,
        decode_evidence,
    )?;
    Ok(PublicationEvidenceEnvelope {
        payload,
        payload_digest,
    })
}

/// Builds the unique digest-bound value for one successful-publication receipt.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_evidence`]
/// enforces or the encoded document exceeds its byte ceiling.
pub fn evidence(input: &PublicationEvidence) -> Result<Value, Error> {
    let payload = evidence_value(input)?;
    let _validated = decode_evidence("$.payload", payload.clone())?;
    crate::bounded_envelope::build(
        payload,
        EVIDENCE_ENVELOPE_SCHEMA,
        EVIDENCE_PAYLOAD_SCHEMA,
        PUBLICATION_DOCUMENT_BYTES,
    )
}

fn decode_evidence(path: &str, value: Value) -> Result<PublicationEvidence, Error> {
    let mut evidence = Obj::new(path, value)?;
    evidence.required("schema", |path, value| {
        de::const_str(path, value, EVIDENCE_PAYLOAD_SCHEMA)
    })?;
    let plan_payload_digest = evidence.required("plan_payload_digest", de::digest)?;
    let facts = decode_facts(&mut evidence)?;
    let deployment = evidence.required("deployment", |path, value| {
        let mut deployment = Obj::new(path, value)?;
        deployment.required("outcome", |path, value| {
            de::const_str(path, value, "succeeded")
        })?;
        let record = deployment.required("record", decode_resource)?;
        let workflow = deployment.required("workflow", decode_resource)?;
        let attempt_path = deployment.field("provider_run_attempt");
        let provider_run_attempt = u64::try_from(de::integer(
            &attempt_path,
            deployment.take("provider_run_attempt")?,
        )?)
        .ok()
        .filter(|attempt| *attempt >= 1)
        .ok_or_else(|| Error::new(&attempt_path, ErrorKind::InvalidValue))?;
        deployment.finish()?;
        Ok(PublicationDeployment {
            record,
            workflow,
            provider_run_attempt,
        })
    })?;
    evidence.finish()?;
    Ok(PublicationEvidence {
        plan_payload_digest,
        producer: facts.producer,
        deployment,
        docs: facts.docs,
        target: facts.target,
        site: facts.site,
        product: facts.product,
    })
}

fn evidence_value(evidence: &PublicationEvidence) -> Result<Value, Error> {
    Ok(object(vec![
        ("schema", text(EVIDENCE_PAYLOAD_SCHEMA)),
        (
            "plan_payload_digest",
            text(&evidence.plan_payload_digest.to_string()),
        ),
        ("producer", producer_value(&evidence.producer)),
        (
            "deployment",
            object(vec![
                ("outcome", text("succeeded")),
                ("record", resource_value(&evidence.deployment.record)),
                ("workflow", resource_value(&evidence.deployment.workflow)),
                (
                    "provider_run_attempt",
                    positive_safe_integer(
                        "$.payload.deployment.provider_run_attempt",
                        evidence.deployment.provider_run_attempt,
                    )?,
                ),
            ]),
        ),
        ("docs", docs_value(&evidence.docs)),
        ("target", target_value(&evidence.target)),
        ("site", site_value(&evidence.site)),
        ("product", resource_value(&evidence.product)),
    ]))
}
