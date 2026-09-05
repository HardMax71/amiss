use serde::{Deserialize, Serialize};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::json;

use super::{
    CompletedSite, DocsCandidate, PUBLICATION_DOCUMENT_BYTES, PublicationProducer,
    PublicationResource, PublicationTarget, PublicationUriKind, validate_facts,
    validate_publication_uri,
};

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/publication-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/publication-evidence-payload";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationEvidenceEnvelope<T = PublicationEvidence> {
    pub schema: EvidenceEnvelopeSchema,
    pub payload: T,
    pub payload_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceEnvelopeSchema {
    #[serde(rename = "amiss/publication-evidence-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationEvidence {
    pub schema: EvidencePayloadSchema,
    pub plan_payload_digest: Digest,
    pub producer: PublicationProducer,
    pub deployment: PublicationDeployment,
    pub docs: DocsCandidate,
    pub target: PublicationTarget,
    pub site: CompletedSite,
    pub product: PublicationResource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidencePayloadSchema {
    #[serde(rename = "amiss/publication-evidence-payload")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationDeployment {
    pub outcome: PublicationOutcome,
    pub record: PublicationResource,
    pub workflow: PublicationResource,
    pub provider_run_attempt: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicationOutcome {
    #[serde(rename = "succeeded")]
    Succeeded,
}

/// Parses one closed, digest-bound successful-publication receipt.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// identity or resource, a non-success outcome, an unsafe run attempt, or a
/// payload digest mismatch.
pub fn parse_evidence(bytes: &[u8]) -> Result<PublicationEvidenceEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: PublicationEvidenceEnvelope = de::deserialize_json(bytes)?;
    if evidence_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Builds the unique digest-bound value for one successful-publication receipt.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_evidence`]
/// enforces or the encoded document exceeds its byte ceiling.
pub fn evidence(input: &PublicationEvidence) -> Result<Vec<u8>, Error> {
    let payload_digest = evidence_payload_digest(input)?;
    let document = PublicationEvidenceEnvelope {
        schema: EvidenceEnvelopeSchema::Current,
        payload: input,
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    Ok(canonical)
}

pub(super) fn evidence_payload_digest(input: &PublicationEvidence) -> Result<Digest, Error> {
    validate_evidence(input)?;
    serde_json_canonicalizer::to_vec(input)
        .map(|canonical| hb(EVIDENCE_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

fn validate_evidence(evidence: &PublicationEvidence) -> Result<(), Error> {
    validate_facts(
        "$.payload",
        &evidence.docs,
        &evidence.target,
        &evidence.site,
        &evidence.product,
        &evidence.producer,
    )?;
    for (field, resource) in [
        ("record", &evidence.deployment.record),
        ("workflow", &evidence.deployment.workflow),
    ] {
        validate_publication_uri(
            &format!("$.payload.deployment.{field}.uri"),
            &resource.uri,
            PublicationUriKind::Resource,
        )?;
    }
    if !(1..=json::MAX_SAFE_INTEGER.unsigned_abs())
        .contains(&evidence.deployment.provider_run_attempt)
    {
        return fail(
            "$.payload.deployment.provider_run_attempt",
            ErrorKind::InvalidValue,
        );
    }
    Ok(())
}
