use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{Error, ErrorKind, fail};
use crate::envelope::Payload;
use crate::model::Digest;

use super::{
    CompletedSite, DocsCandidate, PUBLICATION_DOCUMENT_BYTES, PublicationProducer,
    PublicationResource, PublicationTarget, PublicationUriKind, validate_facts,
    validate_publication_uri,
};

pub const EVIDENCE_ENVELOPE_SCHEMA: &str = "amiss/publication-evidence-envelope";
pub const EVIDENCE_PAYLOAD_SCHEMA: &str = "amiss/publication-evidence-payload";

#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum EvidenceEnvelopeSchema {
    #[default]
    #[strum(serialize = "amiss/publication-evidence-envelope")]
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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum EvidencePayloadSchema {
    #[strum(serialize = "amiss/publication-evidence-payload")]
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

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum PublicationOutcome {
    #[strum(serialize = "succeeded")]
    Succeeded,
}

impl Payload for PublicationEvidence {
    type Schema = EvidenceEnvelopeSchema;
    type Defect = Error;
    const DOMAIN: &'static str = EVIDENCE_PAYLOAD_SCHEMA;
    const DOCUMENT_BYTES: u64 = PUBLICATION_DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), Error> {
        validate_evidence(self)
    }
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
    if !(1..=js_int::MAX_SAFE_UINT).contains(&evidence.deployment.provider_run_attempt) {
        return fail(
            "$.payload.deployment.provider_run_attempt",
            ErrorKind::InvalidValue,
        );
    }
    Ok(())
}
