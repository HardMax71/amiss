mod tests;

use std::collections::BTreeSet;

use amiss_wire::assessment::Nullable;
use amiss_wire::digest::sha256;
use amiss_wire::report::model::{Controls, ReportEnvelope};

use crate::semantic_artifact::InputArtifact;

use super::ArtifactError;

pub(super) fn validate(report: &ReportEnvelope, artifact: &[u8]) -> Result<(), ArtifactError> {
    if artifact.is_empty() {
        return Err(ArtifactError::Corrupt);
    }
    if u64::try_from(artifact.len()).unwrap_or(u64::MAX) > crate::SEMANTIC_INPUT_ARTIFACT_BYTES {
        return Err(ArtifactError::TooLarge);
    }
    let report_evidence = match &report.payload.controls {
        Controls::Resolved(controls) => controls.semantic_evidence.as_deref().unwrap_or_default(),
        Controls::Unavailable(_) => &[],
    };
    let decoded: InputArtifact<'static> =
        amiss_wire::read_json(artifact, crate::SEMANTIC_INPUT_ARTIFACT_BYTES)
            .map_err(|_defect| ArtifactError::Corrupt)?;
    if decoded.inputs.is_empty()
        || decoded.inputs.len() > amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT
    {
        return Err(ArtifactError::Corrupt);
    }

    let mut acquisition_identities = BTreeSet::new();
    let mut candidate_identity = None;
    let mut payload_digests = Vec::with_capacity(decoded.inputs.len());
    for row in decoded.inputs {
        if let Some(identity) = row.acquisition_identity
            && !acquisition_identities.insert(identity)
        {
            return Err(ArtifactError::Corrupt);
        }
        if sha256(&row.template_bytes) != row.template_digest
            || sha256(&row.envelope_bytes) != row.envelope_digest
        {
            return Err(ArtifactError::Corrupt);
        }

        let template = amiss_wire::semantic::parse_template(&row.template_bytes)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let envelope = amiss_wire::semantic::parse(&row.envelope_bytes)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let candidate = envelope.payload.subject.candidate_identity_digest;
        if envelope.payload_digest != row.payload_digest
            || envelope.payload.subject.source_report_payload_digest != Nullable::Null
            || candidate_identity.is_some_and(|expected| expected != candidate)
        {
            return Err(ArtifactError::Corrupt);
        }
        candidate_identity = Some(candidate);
        let document = amiss_wire::semantic::bind_template(&template, candidate)
            .map_err(|_defect| ArtifactError::Corrupt)?;
        let mut rebound = Vec::new();
        amiss_wire::write_json(
            &document,
            &mut rebound,
            amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES,
        )
        .map_err(|_defect| ArtifactError::Corrupt)?;
        if rebound != row.envelope_bytes.as_ref() {
            return Err(ArtifactError::Corrupt);
        }
        payload_digests.push(row.payload_digest);
    }
    if payload_digests
        .windows(2)
        .any(|pair| matches!(pair, [left, right] if left >= right))
        || !payload_digests.iter().copied().eq(report_evidence
            .iter()
            .map(|evidence| evidence.payload_digest))
    {
        return Err(ArtifactError::Corrupt);
    }
    Ok(())
}
