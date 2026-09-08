mod tests;

use std::collections::BTreeSet;

use amiss_wire::assessment::Nullable;
use amiss_wire::digest::sha256;
use amiss_wire::report::model::{Controls, ReportEnvelope};

use crate::BoundSemanticEvidence;

use super::ArtifactError;

pub(super) fn validate(
    report: &ReportEnvelope,
    bound: &BoundSemanticEvidence,
) -> Result<(), ArtifactError> {
    let artifact = bound.artifact.as_ref().ok_or(ArtifactError::Corrupt)?;
    let report_evidence = match &report.payload.controls {
        Controls::Resolved(controls) => controls.semantic_evidence.as_deref().unwrap_or_default(),
        Controls::Unavailable(_) => &[],
    };
    if artifact.inputs.is_empty()
        || artifact.inputs.len() > amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT
        || artifact.inputs.len() != bound.supplied.len()
    {
        return Err(ArtifactError::Corrupt);
    }

    let mut acquisition_identities = BTreeSet::new();
    let mut candidate_identity = None;
    let mut payload_digests = Vec::with_capacity(artifact.inputs.len());
    for (row, supplied) in artifact.inputs.iter().zip(&bound.supplied) {
        if let Some(identity) = &row.acquisition_identity
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
        let envelope = &supplied.value;
        let candidate = envelope.payload.subject.candidate_identity_digest;
        if envelope.payload_digest != row.payload_digest
            || envelope.payload.subject.source_report_payload_digest != Nullable::Null
            || candidate_identity.is_some_and(|expected| expected != candidate)
            || supplied.expected_context_digest != envelope.payload.producer.context_digest
            || template.producer != envelope.payload.producer
            || template.complete != envelope.payload.complete
            || template.observations.as_ref() != envelope.payload.observations.as_slice()
        {
            return Err(ArtifactError::Corrupt);
        }
        amiss_wire::semantic::validate(envelope).map_err(|_defect| ArtifactError::Corrupt)?;
        candidate_identity = Some(candidate);
        let mut encoded = Vec::new();
        amiss_wire::write_json(
            envelope,
            &mut encoded,
            amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES,
        )
        .map_err(|_defect| ArtifactError::Corrupt)?;
        if encoded != row.envelope_bytes.as_ref() {
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
