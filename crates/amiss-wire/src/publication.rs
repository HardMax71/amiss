use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::codec::{self, Document, Envelope, Schema};
use crate::de::{Error, ErrorKind, fail};
use crate::digest::Digest;
use crate::json::Value;
use crate::model::{ArtifactId, ObjectFormat, Oid, RepositoryIdentity};

mod assessment;
mod evidence;

pub use crate::assessment::AssessmentVerdict as PublicationVerdict;
pub use assessment::{
    ASSESSMENT_ENVELOPE_SCHEMA, ASSESSMENT_PAYLOAD_SCHEMA, PublicationAssessment,
    PublicationAssessmentEnvelope, PublicationReason, assess, parse_assessment,
};
pub use evidence::{
    EVIDENCE_ENVELOPE_SCHEMA, EVIDENCE_PAYLOAD_SCHEMA, PublicationDeployment, PublicationEvidence,
    PublicationEvidenceEnvelope, PublicationOutcome, evidence, parse_evidence,
};

pub const PLAN_ENVELOPE_SCHEMA: &str = "amiss/publication-plan-envelope";
pub const PLAN_PAYLOAD_SCHEMA: &str = "amiss/publication-plan-payload";
pub const PUBLICATION_DOCUMENT_BYTES: u64 = 65_536;
pub const PUBLICATION_URI_BYTES: usize = 16_384;

pub type PublicationPlanEnvelope = Envelope<PublicationPlan>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationPlan {
    pub schema: Schema<Self>,
    pub report_payload_digest: Digest,
    #[garde(dive)]
    pub docs: DocsCandidate,
    #[garde(dive)]
    pub target: PublicationTarget,
    #[garde(dive)]
    pub site: CompletedSite,
    #[garde(dive)]
    pub product: PublicationResource,
    #[garde(dive)]
    pub producer: PublicationProducer,
    pub relation: PublicationRelation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct DocsCandidate {
    pub repository: RepositoryIdentity,
    pub object_format: ObjectFormat,
    #[serde(rename = "commit_oid")]
    pub commit: Oid,
    #[serde(rename = "tree_oid")]
    pub tree: Oid,
    pub candidate_identity_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationTarget {
    pub provider: ArtifactId,
    pub instance: ArtifactId,
    pub environment: ArtifactId,
    pub channel: ArtifactId,
    #[garde(custom(valid_canonical_url))]
    pub canonical_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct CompletedSite {
    #[garde(dive)]
    pub artifact: PublicationResource,
    pub input_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationResource {
    #[garde(custom(resource_uri))]
    pub uri: String,
    pub digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationProducer {
    pub identity: ArtifactId,
    #[garde(custom(open_identity))]
    pub version: String,
    pub context_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationRelation {
    pub identity: ArtifactId,
    pub context_digest: Digest,
}

/// Reads a closed, bounded publication plan and verifies its payload digest.
///
/// # Errors
///
/// The document violates its wire shape, field constraints, or digest binding.
pub fn parse_plan(bytes: &[u8]) -> Result<PublicationPlanEnvelope, Error> {
    PublicationPlanEnvelope::parse(bytes)
}

/// Seals a publication plan after checking its field and object-format laws.
///
/// # Errors
///
/// The plan violates its contract or exceeds the document byte ceiling.
pub fn plan(input: &PublicationPlan) -> Result<Value, Error> {
    codec::seal_value(input)
}

impl Document for PublicationPlan {
    const PAYLOAD_SCHEMA: &'static str = PLAN_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = PLAN_ENVELOPE_SCHEMA;
    const LIMIT: u64 = PUBLICATION_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        self.docs.check(&format!("{root}.docs"))
    }
}

impl DocsCandidate {
    pub(crate) fn check(&self, root: &str) -> Result<(), Error> {
        for (name, oid) in [("commit_oid", &self.commit), ("tree_oid", &self.tree)] {
            if oid.object_format() != self.object_format {
                return fail(&format!("{root}.{name}"), ErrorKind::InvalidValue);
            }
        }
        Ok(())
    }
}

pub(crate) fn open_identity<C>(value: &str, _context: &C) -> garde::Result {
    codec::rule(
        crate::semantic::producer_version_valid(value),
        "invalid open identity",
    )
}

pub(crate) fn bounded_text(value: &str, limit: usize) -> bool {
    !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control)
}

fn valid_canonical_url<C>(value: &str, _context: &C) -> garde::Result {
    codec::rule(
        bounded_text(value, PUBLICATION_URI_BYTES) && canonical_url_valid(value),
        "invalid canonical URL",
    )
}

fn resource_uri<C>(value: &str, _context: &C) -> garde::Result {
    codec::rule(
        bounded_text(value, PUBLICATION_URI_BYTES) && resource_uri_valid(value),
        "invalid resource URI",
    )
}

fn canonical_url_valid(raw: &str) -> bool {
    raw.strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .is_some_and(authority_valid)
        && !raw.contains(['?', '#'])
        && crate::uri::http_destination_valid(raw)
}

fn authority_valid(authority: &str) -> bool {
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
            && authority
                .rsplit_once(':')
                .map_or(!authority.is_empty(), |(host, port)| {
                    !host.is_empty() && !host.contains(':') && port.parse::<u16>().is_ok()
                })
    }
}

fn resource_uri_valid(raw: &str) -> bool {
    let (without_query, query) = raw
        .split_once('?')
        .map_or((raw, None), |(path, query)| (path, Some(query)));
    crate::uri::scheme(without_query).is_some_and(|scheme| {
        scheme.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'+' | b'.' | b'-')
        }) && without_query
            .get(scheme.len().saturating_add(1)..)
            .is_some_and(|body| !body.is_empty())
            && !raw.contains('#')
            && crate::uri::absolute_valid(without_query, scheme, query)
    })
}
