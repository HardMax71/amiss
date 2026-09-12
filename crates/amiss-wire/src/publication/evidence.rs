use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::codec::{self, Document, Envelope, MAX_SAFE_INTEGER, Schema};
use crate::de::Error;
use crate::digest::Digest;
use crate::json::Value;

use super::{
    CompletedSite, DocsCandidate, PUBLICATION_DOCUMENT_BYTES, PublicationProducer,
    PublicationResource, PublicationTarget,
};

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/publication-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/publication-evidence-payload";

pub type PublicationEvidenceEnvelope = Envelope<PublicationEvidence>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationEvidence {
    pub schema: Schema<Self>,
    pub plan_payload_digest: Digest,
    #[garde(dive)]
    pub producer: PublicationProducer,
    #[garde(dive)]
    pub deployment: PublicationDeployment,
    #[garde(dive)]
    pub docs: DocsCandidate,
    #[garde(dive)]
    pub target: PublicationTarget,
    #[garde(dive)]
    pub site: CompletedSite,
    #[garde(dive)]
    pub product: PublicationResource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[garde(allow_unvalidated)]
pub struct PublicationDeployment {
    pub outcome: PublicationOutcome,
    #[garde(dive)]
    pub record: PublicationResource,
    #[garde(dive)]
    pub workflow: PublicationResource,
    #[garde(range(min = 1, max = MAX_SAFE_INTEGER))]
    pub provider_run_attempt: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublicationOutcome {
    #[default]
    Succeeded,
}

/// Reads a closed, bounded successful-publication receipt.
///
/// # Errors
///
/// The receipt violates its shape, constraints, or payload digest binding.
pub fn parse_evidence(bytes: &[u8]) -> Result<PublicationEvidenceEnvelope, Error> {
    PublicationEvidenceEnvelope::parse(bytes)
}

/// Seals a successful-publication receipt after validating its field laws.
///
/// # Errors
///
/// The receipt violates its contract or exceeds the document byte ceiling.
pub fn evidence(input: &PublicationEvidence) -> Result<Value, Error> {
    codec::seal_value(input)
}

impl Document for PublicationEvidence {
    const PAYLOAD_SCHEMA: &'static str = EVIDENCE_PAYLOAD_SCHEMA;
    const ENVELOPE_SCHEMA: &'static str = EVIDENCE_ENVELOPE_SCHEMA;
    const LIMIT: u64 = PUBLICATION_DOCUMENT_BYTES;

    fn check(&self, root: &str) -> Result<(), Error> {
        self.docs.check(&format!("{root}.docs"))
    }
}
