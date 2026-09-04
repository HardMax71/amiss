mod tests;

use std::collections::BTreeSet;

use amiss_wire::assessment::Nullable;
use amiss_wire::digest::{Digest, sha256};
use amiss_wire::json;
use amiss_wire::model::ArtifactId;
use base64::Engine as _;
use serde::Deserialize;

use super::ArtifactError;

const INPUT_ARTIFACT_SCHEMA: &str = "amiss/controller-semantic-input-artifact-v1";

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
#[serde(deny_unknown_fields)]
struct InputArtifact {
    inputs: Vec<InputArtifactRow>,
    schema: String,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
#[serde(deny_unknown_fields)]
struct InputArtifactRow {
    acquisition_identity: Option<String>,
    envelope_bytes_base64: String,
    envelope_digest: String,
    payload_digest: String,
    template_bytes_base64: String,
    template_digest: String,
}

pub(super) fn validate(report: &[u8], artifact: &[u8]) -> Result<(), ArtifactError> {
    if artifact.is_empty() {
        return Err(ArtifactError::Corrupt);
    }
    if u64::try_from(artifact.len()).unwrap_or(u64::MAX) > crate::SEMANTIC_INPUT_ARTIFACT_BYTES {
        return Err(ArtifactError::TooLarge);
    }
    let report_digests = report_digests(report)?;
    let decoded: InputArtifact =
        serde_json::from_slice(artifact).map_err(|_defect| ArtifactError::Corrupt)?;
    if decoded.schema != INPUT_ARTIFACT_SCHEMA
        || decoded.inputs.is_empty()
        || decoded.inputs.len() > amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT
    {
        return Err(ArtifactError::Corrupt);
    }

    let mut acquisition_identities = BTreeSet::new();
    let mut candidate_identity = None;
    let mut payload_digests = Vec::with_capacity(decoded.inputs.len());
    for row in decoded.inputs {
        if let Some(identity) = row.acquisition_identity
            && !ArtifactId::new(identity)
                .is_some_and(|identity| acquisition_identities.insert(identity))
        {
            return Err(ArtifactError::Corrupt);
        }
        let template_bytes = base64::engine::general_purpose::STANDARD
            .decode(row.template_bytes_base64)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let envelope_bytes = base64::engine::general_purpose::STANDARD
            .decode(row.envelope_bytes_base64)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let template_digest =
            Digest::from_wire(&row.template_digest).ok_or(ArtifactError::Corrupt)?;
        let envelope_digest =
            Digest::from_wire(&row.envelope_digest).ok_or(ArtifactError::Corrupt)?;
        let payload_digest =
            Digest::from_wire(&row.payload_digest).ok_or(ArtifactError::Corrupt)?;
        if sha256(&template_bytes) != template_digest || sha256(&envelope_bytes) != envelope_digest
        {
            return Err(ArtifactError::Corrupt);
        }

        let template = amiss_wire::semantic::parse_template(&template_bytes)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let envelope = amiss_wire::semantic::parse(&envelope_bytes)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let candidate = envelope.payload.subject.candidate_identity_digest;
        if envelope.payload_digest != payload_digest
            || envelope.payload.subject.source_report_payload_digest != Nullable::Null
            || candidate_identity.is_some_and(|expected| expected != candidate)
        {
            return Err(ArtifactError::Corrupt);
        }
        candidate_identity = Some(candidate);
        let rebound = amiss_wire::semantic::bind_template(&template, candidate)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        if rebound != envelope_bytes {
            return Err(ArtifactError::Corrupt);
        }
        payload_digests.push(payload_digest);
    }
    if payload_digests
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left >= right))
        || payload_digests != report_digests
    {
        return Err(ArtifactError::Corrupt);
    }
    Ok(())
}

fn report_digests(report: &[u8]) -> Result<Vec<Digest>, ArtifactError> {
    let envelope = json::parse(report).map_err(|_defect| ArtifactError::Corrupt)?;
    let (payload, _digest, _verdict) = amiss_wire::report::validate_envelope(&envelope)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let amiss_wire::report::model::Controls::Resolved(controls) = payload.controls else {
        return Ok(Vec::new());
    };
    Ok(controls
        .semantic_evidence
        .unwrap_or_default()
        .into_iter()
        .map(|evidence| evidence.payload_digest)
        .collect())
}
