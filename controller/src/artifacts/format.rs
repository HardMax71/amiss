use std::time::Duration;

use amiss_wire::digest::{Digest, hb};
use amiss_wire::publication::{PUBLICATION_DOCUMENT_BYTES, PublicationVerdict};
use amiss_wire::relation::{RELATION_DOCUMENT_BYTES, RelationVerdict};
use amiss_wire::report::MACHINE_JSON_BYTES;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::{ArtifactError, ArtifactReference, ArtifactStoreConfig};
use crate::{ControllerEvaluationId, ExternalTally};

pub(super) const ROOT_SCHEMA: &str = "amiss/controller-artifact-root-v1";
const RECORD_SCHEMA: &str = "amiss/controller-artifact-record-v1";
const ROOT_DOMAIN: &str = "amiss/controller-artifact-root-payload-v1";
const RECORD_DOMAIN: &str = "amiss/controller-artifact-record-payload-v1";
const ID_DOMAIN: &str = "amiss/controller-artifact-identity-v1";

pub(super) const MAX_ROOT_BYTES: u64 = 8_192;
pub(super) const MAX_RECORD_METADATA_BYTES: u64 = 16_384;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Root {
    pub(super) schema: String,
    pub(super) base_url: String,
    pub(super) retention_millis: u64,
    pub(super) max_records: u64,
    pub(super) max_bytes: u64,
    pub(super) max_record_bytes: u64,
    pub(super) clock_high_water_unix_millis: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Blob {
    pub(super) digest: String,
    pub(super) length: u64,
}

impl Blob {
    pub(super) fn new(bytes: &[u8]) -> Result<Self, ArtifactError> {
        Self::from_digest(bytes, amiss_wire::digest::sha256(bytes))
    }

    pub(super) fn from_digest(bytes: &[u8], digest: Digest) -> Result<Self, ArtifactError> {
        let length = u64::try_from(bytes.len()).map_err(|_defect| ArtifactError::TooLarge)?;
        if bytes.is_empty() {
            return Err(ArtifactError::Corrupt);
        }
        if length > MACHINE_JSON_BYTES {
            return Err(ArtifactError::TooLarge);
        }
        Ok(Self {
            digest: digest.to_string(),
            length,
        })
    }

    pub(super) fn parsed_digest(&self) -> Result<Digest, ArtifactError> {
        Digest::from_wire(&self.digest).ok_or(ArtifactError::Corrupt)
    }

    fn valid(&self) -> bool {
        self.length > 0
            && self.length <= MACHINE_JSON_BYTES
            && Digest::from_wire(&self.digest).is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    schema: String,
    pub(super) id: String,
    pub(super) evaluation_id: String,
    pub(super) created_at_unix_millis: i64,
    pub(super) expires_at_unix_millis: i64,
    pub(super) report: Blob,
    pub(super) plan: Option<Blob>,
    pub(super) evidence: Option<Blob>,
    pub(super) assessment: Option<Blob>,
    external_tally: Option<ExternalTally>,
    pub(super) external_incomplete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) semantic: Option<Blob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) publication_audit: Option<SidecarAudit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) relation_audit: Option<SidecarAudit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SidecarAudit {
    pub(super) plan: Blob,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) evidence: Option<Blob>,
    pub(super) assessment: Blob,
    pub(super) verdict: String,
}

pub(super) struct RecordInput {
    pub(super) report: Blob,
    pub(super) plan: Option<Blob>,
    pub(super) evidence: Option<Blob>,
    pub(super) assessment: Option<Blob>,
    pub(super) external_tally: Option<ExternalTally>,
    pub(super) external_incomplete: bool,
    pub(super) semantic: Option<Blob>,
    pub(super) publication_audit: Option<SidecarAudit>,
    pub(super) relation_audit: Option<SidecarAudit>,
}

impl Record {
    pub(super) fn new(
        evaluation_id: &ControllerEvaluationId,
        created_at_unix_millis: i64,
        retention: Duration,
        input: RecordInput,
    ) -> Result<Self, ArtifactError> {
        let retention_millis =
            i64::try_from(millis(retention)?).map_err(|_defect| ArtifactError::Clock)?;
        let expires_at_unix_millis = created_at_unix_millis
            .checked_add(retention_millis)
            .ok_or(ArtifactError::Clock)?;
        let mut record = Self {
            schema: RECORD_SCHEMA.to_owned(),
            id: String::new(),
            evaluation_id: evaluation_id.as_str().to_owned(),
            created_at_unix_millis,
            expires_at_unix_millis,
            report: input.report,
            plan: input.plan,
            evidence: input.evidence,
            assessment: input.assessment,
            external_tally: input.external_tally,
            external_incomplete: input.external_incomplete,
            semantic: input.semantic,
            publication_audit: input.publication_audit,
            relation_audit: input.relation_audit,
        };
        record.id = record.expected_id()?;
        record.validate(retention)?;
        Ok(record)
    }

    pub(super) fn validate(&self, retention: Duration) -> Result<(), ArtifactError> {
        let retention_millis =
            i64::try_from(millis(retention)?).map_err(|_defect| ArtifactError::Corrupt)?;
        let valid_chain = self.assessment.is_some() == self.external_tally.is_some()
            && (!self.external_incomplete || self.assessment.is_none())
            && self
                .assessment
                .as_ref()
                .is_none_or(|_assessment| self.plan.is_some() && self.evidence.is_some());
        let sidecar_valid = match (&self.publication_audit, &self.relation_audit) {
            (None, None) => true,
            (Some(audit), None) => valid_sidecar(
                audit,
                PUBLICATION_DOCUMENT_BYTES,
                &PublicationVerdict::Unproven,
            ),
            (None, Some(audit)) => {
                valid_sidecar(audit, RELATION_DOCUMENT_BYTES, &RelationVerdict::Unproven)
            }
            (Some(_publication), Some(_relation)) => false,
        };
        let has_sidecar = self.publication_audit.is_some() || self.relation_audit.is_some();
        let sidecar_isolated = !has_sidecar
            || self.semantic.is_none()
                && self.plan.is_none()
                && self.evidence.is_none()
                && self.assessment.is_none()
                && self.external_tally.is_none()
                && !self.external_incomplete;
        if self.schema != RECORD_SCHEMA
            || !valid_id(&self.id)
            || super::evaluation_id(&self.evaluation_id).is_err()
            || self.created_at_unix_millis < 0
            || self.created_at_unix_millis.checked_add(retention_millis)
                != Some(self.expires_at_unix_millis)
            || !self.report.valid()
            || self.plan.as_ref().is_some_and(|blob| !blob.valid())
            || self.evidence.as_ref().is_some_and(|blob| !blob.valid())
            || self.assessment.as_ref().is_some_and(|blob| !blob.valid())
            || self.semantic.as_ref().is_some_and(|blob| {
                !blob.valid() || blob.length > crate::SEMANTIC_INPUT_ARTIFACT_BYTES
            })
            || !valid_chain
            || !sidecar_valid
            || !sidecar_isolated
            || !self.expected_id().is_ok_and(|expected| expected == self.id)
        {
            return Err(ArtifactError::Corrupt);
        }
        Ok(())
    }

    pub(super) fn reference(
        &self,
        config: &ArtifactStoreConfig,
    ) -> Result<ArtifactReference, ArtifactError> {
        super::checked_reference(ArtifactReference {
            id: self.id.clone(),
            locator: format!("{}/{}/report", config.base_url, self.id),
            expires_at_unix_millis: self.expires_at_unix_millis,
            report_digest: self.report.parsed_digest()?,
            semantic_digest: self
                .semantic
                .as_ref()
                .map(Blob::parsed_digest)
                .transpose()?,
            assessment_digest: self
                .assessment
                .as_ref()
                .map(Blob::parsed_digest)
                .transpose()?,
            external_tally: self.external_tally,
            external_incomplete: self.external_incomplete,
        })
        .ok_or(ArtifactError::Corrupt)
    }

    pub(super) fn blobs(&self) -> impl Iterator<Item = (super::ArtifactComponent, &Blob)> {
        [
            (super::ArtifactComponent::Report, Some(&self.report)),
            (super::ArtifactComponent::Plan, self.plan.as_ref()),
            (super::ArtifactComponent::Evidence, self.evidence.as_ref()),
            (
                super::ArtifactComponent::Assessment,
                self.assessment.as_ref(),
            ),
            (super::ArtifactComponent::Semantic, self.semantic.as_ref()),
            (
                super::ArtifactComponent::PublicationPlan,
                self.publication_audit.as_ref().map(|audit| &audit.plan),
            ),
            (
                super::ArtifactComponent::PublicationEvidence,
                self.publication_audit
                    .as_ref()
                    .and_then(|audit| audit.evidence.as_ref()),
            ),
            (
                super::ArtifactComponent::PublicationAssessment,
                self.publication_audit
                    .as_ref()
                    .map(|audit| &audit.assessment),
            ),
            (
                super::ArtifactComponent::RelationPlan,
                self.relation_audit.as_ref().map(|audit| &audit.plan),
            ),
            (
                super::ArtifactComponent::RelationEvidence,
                self.relation_audit
                    .as_ref()
                    .and_then(|audit| audit.evidence.as_ref()),
            ),
            (
                super::ArtifactComponent::RelationAssessment,
                self.relation_audit.as_ref().map(|audit| &audit.assessment),
            ),
        ]
        .into_iter()
        .filter_map(|(component, blob)| blob.map(|blob| (component, blob)))
    }

    fn expected_id(&self) -> Result<String, ArtifactError> {
        #[derive(Serialize)]
        struct Identity<'a> {
            evaluation_id: &'a str,
            report: &'a Blob,
            plan: &'a Option<Blob>,
            evidence: &'a Option<Blob>,
            assessment: &'a Option<Blob>,
            external_tally: &'a Option<ExternalTally>,
            external_incomplete: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            semantic: &'a Option<Blob>,
            #[serde(skip_serializing_if = "Option::is_none")]
            publication_audit: &'a Option<SidecarAudit>,
            #[serde(skip_serializing_if = "Option::is_none")]
            relation_audit: &'a Option<SidecarAudit>,
        }
        let identity = Identity {
            evaluation_id: &self.evaluation_id,
            report: &self.report,
            plan: &self.plan,
            evidence: &self.evidence,
            assessment: &self.assessment,
            external_tally: &self.external_tally,
            external_incomplete: self.external_incomplete,
            semantic: &self.semantic,
            publication_audit: &self.publication_audit,
            relation_audit: &self.relation_audit,
        };
        let bytes = serde_json::to_vec(&identity).map_err(|_defect| ArtifactError::Corrupt)?;
        Ok(hex::encode(hb(ID_DOMAIN, &bytes).as_bytes()))
    }
}

fn valid_sidecar<V>(audit: &SidecarAudit, maximum: u64, unproven: &V) -> bool
where
    V: std::str::FromStr + PartialEq,
{
    audit.plan.valid()
        && audit.plan.length <= maximum
        && audit
            .evidence
            .as_ref()
            .is_none_or(|blob| blob.valid() && blob.length <= maximum)
        && audit.assessment.valid()
        && audit.assessment.length <= maximum
        && audit
            .verdict
            .parse::<V>()
            .is_ok_and(|verdict| &verdict == unproven || audit.evidence.is_some())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    payload: T,
    payload_digest: String,
}

pub(super) fn encode_root(root: &Root) -> Result<Vec<u8>, ArtifactError> {
    encode(root, ROOT_DOMAIN, MAX_ROOT_BYTES)
}

pub(super) fn decode_root(bytes: &[u8]) -> Result<Root, ArtifactError> {
    decode(bytes, ROOT_DOMAIN, MAX_ROOT_BYTES)
}

pub(super) fn encode_record(record: &Record) -> Result<Vec<u8>, ArtifactError> {
    encode(record, RECORD_DOMAIN, MAX_RECORD_METADATA_BYTES)
}

pub(super) fn decode_record(bytes: &[u8]) -> Result<Record, ArtifactError> {
    decode(bytes, RECORD_DOMAIN, MAX_RECORD_METADATA_BYTES)
}

fn encode<T: Serialize>(value: &T, domain: &str, maximum: u64) -> Result<Vec<u8>, ArtifactError> {
    let payload = serde_json::to_vec(value).map_err(|_defect| ArtifactError::Corrupt)?;
    let envelope = Envelope {
        payload: value,
        payload_digest: hb(domain, &payload).to_string(),
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|_defect| ArtifactError::Corrupt)?;
    (u64::try_from(bytes.len())
        .ok()
        .is_some_and(|length| length <= maximum))
    .then_some(bytes)
    .ok_or(ArtifactError::Corrupt)
}

fn decode<T>(bytes: &[u8], domain: &str, maximum: u64) -> Result<T, ArtifactError>
where
    T: DeserializeOwned + Serialize,
{
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum {
        return Err(ArtifactError::Corrupt);
    }
    let envelope: Envelope<T> =
        serde_json::from_slice(bytes).map_err(|_defect| ArtifactError::Corrupt)?;
    let canonical = serde_json::to_vec(&envelope).map_err(|_defect| ArtifactError::Corrupt)?;
    if canonical != bytes {
        return Err(ArtifactError::Corrupt);
    }
    let payload =
        serde_json::to_vec(&envelope.payload).map_err(|_defect| ArtifactError::Corrupt)?;
    (hb(domain, &payload).to_string() == envelope.payload_digest)
        .then_some(envelope.payload)
        .ok_or(ArtifactError::Corrupt)
}

pub(super) fn millis(duration: Duration) -> Result<u64, ArtifactError> {
    u64::try_from(duration.as_millis()).map_err(|_defect| ArtifactError::Configuration)
}

pub(super) fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
