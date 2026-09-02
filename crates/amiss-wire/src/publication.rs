use serde::{Deserialize, Serialize};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::json::{self, Value};
use crate::model::{ArtifactId, ObjectFormat, Oid, RepositoryIdentity};

mod assessment;
mod evidence;

pub use crate::assessment::AssessmentVerdict as PublicationVerdict;
pub use assessment::{
    ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, AssessmentEnvelopeSchema,
    AssessmentPayloadSchema, PublicationAssessment, PublicationAssessmentEnvelope,
    PublicationReason, assess, parse_assessment,
};
pub use evidence::{
    EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA, EvidenceEnvelopeSchema,
    EvidencePayloadSchema, PublicationDeployment, PublicationEvidence, PublicationEvidenceEnvelope,
    PublicationOutcome, evidence, parse_evidence,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/publication-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/publication-plan-payload";
pub const PUBLICATION_DOCUMENT_BYTES: u64 = 65_536;
pub const PUBLICATION_URI_BYTES: usize = 16_384;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationPlanEnvelope {
    pub schema: PlanEnvelopeSchema,
    pub payload: PublicationPlan,
    pub payload_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanEnvelopeSchema {
    #[serde(rename = "amiss/publication-plan-envelope")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationPlan {
    pub schema: PlanPayloadSchema,
    pub report_payload_digest: Digest,
    pub docs: DocsCandidate,
    pub target: PublicationTarget,
    pub site: CompletedSite,
    pub product: PublicationResource,
    pub producer: PublicationProducer,
    pub relation: PublicationRelation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanPayloadSchema {
    #[serde(rename = "amiss/publication-plan-payload")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocsCandidate {
    pub repository: RepositoryIdentity,
    pub object_format: ObjectFormat,
    #[serde(rename = "commit_oid")]
    pub commit: Oid,
    #[serde(rename = "tree_oid")]
    pub tree: Oid,
    pub candidate_identity_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationTarget {
    pub provider: ArtifactId,
    pub instance: ArtifactId,
    pub environment: ArtifactId,
    pub channel: ArtifactId,
    pub canonical_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedSite {
    pub artifact: PublicationResource,
    pub input_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationResource {
    pub uri: String,
    pub digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationProducer {
    pub identity: ArtifactId,
    pub version: String,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRelation {
    pub identity: ArtifactId,
    pub context_digest: Digest,
}

/// Parses one closed, digest-bound publication plan.
///
/// # Errors
///
/// Fails on oversized or malformed strict JSON, an unknown field, an invalid
/// identity or URI, inconsistent Git object formats, or a payload digest mismatch.
pub fn parse_plan(bytes: &[u8]) -> Result<PublicationPlanEnvelope, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let document: PublicationPlanEnvelope = de::deserialize_json(bytes)?;
    if plan_payload_digest(&document.payload)? != document.payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok(document)
}

/// Builds the unique digest-bound value for one publication plan.
///
/// # Errors
///
/// Fails when a public field violates the same closed grammar [`parse_plan`]
/// enforces or the encoded document exceeds its byte ceiling.
pub fn plan(input: &PublicationPlan) -> Result<Value, Error> {
    let payload_digest = plan_payload_digest(input)?;
    let document = PublicationPlanEnvelope {
        schema: PlanEnvelopeSchema::Current,
        payload: input.clone(),
        payload_digest,
    };
    let canonical = serde_json_canonicalizer::to_vec(&document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > PUBLICATION_DOCUMENT_BYTES {
        return fail("$", ErrorKind::LimitExceeded);
    }
    json::parse(&canonical).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

pub(super) fn plan_payload_digest(input: &PublicationPlan) -> Result<Digest, Error> {
    validate_facts(
        "$.payload",
        &input.docs,
        &input.target,
        &input.site,
        &input.product,
        &input.producer,
    )?;
    serde_json_canonicalizer::to_vec(input)
        .map(|canonical| hb(PLAN_PAYLOAD_SCHEMA, &canonical))
        .map_err(|_defect| Error::new("$.payload", ErrorKind::InvalidValue))
}

pub(crate) fn decode_identity(path: &str, value: Value) -> Result<ArtifactId, Error> {
    let raw = de::string(path, value)?;
    ArtifactId::new(raw).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

#[derive(Clone, Copy)]
pub(crate) enum PublicationUriKind {
    CanonicalUrl,
    Resource,
}

pub(crate) fn validate_publication_uri(
    path: &str,
    raw: &str,
    kind: PublicationUriKind,
) -> Result<(), Error> {
    let grammar_valid = match kind {
        PublicationUriKind::CanonicalUrl => {
            let authority = raw
                .strip_prefix("https://")
                .and_then(|rest| rest.split('/').next());
            let authority_valid = authority.is_some_and(|authority| {
                if let Some(bracketed) = authority.strip_prefix('[') {
                    bracketed.split_once(']').is_some_and(|(host, port)| {
                        !host.is_empty()
                            && (port.is_empty()
                                || port
                                    .strip_prefix(':')
                                    .is_some_and(|port| port.parse::<u16>().is_ok()))
                    })
                } else {
                    !authority.contains(['[', ']', '@'])
                        && authority.rsplit_once(':').map_or(
                            !authority.is_empty(),
                            |(host, port)| {
                                !host.is_empty()
                                    && !host.contains(':')
                                    && port.parse::<u16>().is_ok()
                            },
                        )
                }
            });
            authority_valid && !raw.contains('?') && crate::uri::http_destination_valid(raw)
        }
        PublicationUriKind::Resource => {
            let (without_query, query) = raw
                .split_once('?')
                .map_or((raw, None), |(path, query)| (path, Some(query)));
            crate::uri::scheme(without_query).is_some_and(|scheme| {
                scheme.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'+' | b'.' | b'-')
                }) && without_query
                    .get(scheme.len().saturating_add(1)..)
                    .is_some_and(|body| !body.is_empty())
                    && crate::uri::absolute_valid(without_query, scheme, query)
            })
        }
    };
    (raw.len() <= PUBLICATION_URI_BYTES && !raw.contains('#') && grammar_valid)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn validate_facts(
    path: &str,
    docs: &DocsCandidate,
    target: &PublicationTarget,
    site: &CompletedSite,
    product: &PublicationResource,
    producer: &PublicationProducer,
) -> Result<(), Error> {
    validate_docs(&format!("{path}.docs"), docs)?;
    validate_publication_uri(
        &format!("{path}.target.canonical_url"),
        &target.canonical_url,
        PublicationUriKind::CanonicalUrl,
    )?;
    for (field, resource) in [("site.artifact", &site.artifact), ("product", product)] {
        validate_publication_uri(
            &format!("{path}.{field}.uri"),
            &resource.uri,
            PublicationUriKind::Resource,
        )?;
    }
    validate_producer(&format!("{path}.producer"), producer)
}

pub(crate) fn validate_docs(path: &str, docs: &DocsCandidate) -> Result<(), Error> {
    if RepositoryIdentity::new(
        docs.repository.host().to_owned(),
        docs.repository.owner().to_owned(),
        docs.repository.name().to_owned(),
    )
    .as_ref()
        != Some(&docs.repository)
    {
        return fail(&format!("{path}.repository"), ErrorKind::InvalidValue);
    }
    for (field, oid) in [("commit_oid", &docs.commit), ("tree_oid", &docs.tree)] {
        if oid.object_format() != docs.object_format {
            return fail(&format!("{path}.{field}"), ErrorKind::InvalidValue);
        }
    }
    Ok(())
}

pub(crate) fn validate_producer(path: &str, producer: &PublicationProducer) -> Result<(), Error> {
    if !crate::semantic::producer_version_valid(&producer.version) {
        return fail(&format!("{path}.version"), ErrorKind::InvalidValue);
    }
    Ok(())
}
