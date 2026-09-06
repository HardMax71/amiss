mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::digest::{Digest, sha256};
use amiss_wire::model::ArtifactId;
use amiss_wire::requests::SuppliedSemanticEvidence;
use base64::Engine as _;

use super::plan::normalized_expectations;
use super::{
    AcquiredSemanticTemplate, BootstrapJobError, BoundSemanticEvidence,
    SEMANTIC_INPUT_ARTIFACT_BYTES, SemanticEvidenceExpectation, SemanticEvidenceTemplate,
};
use crate::semantic_artifact::{InputArtifact, InputArtifactRow, InputArtifactSchema};

struct BoundInput {
    payload_digest: Digest,
    supplied: SuppliedSemanticEvidence,
    acquisition_identity: Option<ArtifactId>,
    template_bytes: Arc<[u8]>,
    template_digest: Digest,
    envelope_bytes: Vec<u8>,
    envelope_digest: Digest,
}

/// Binds controller-produced and independently acquired templates to one exact candidate.
/// The returned audit artifact retains every source byte and derived envelope in payload order.
///
/// # Errors
///
/// A template is malformed, exceeds a limit, disagrees with its planned acquisition identity,
/// or collides with another derived envelope.
pub fn bind_semantic_evidence(
    templates: &[SemanticEvidenceTemplate],
    expectations: &[SemanticEvidenceExpectation],
    acquired: &[AcquiredSemanticTemplate],
    candidate_identity_digest: Digest,
) -> Result<BoundSemanticEvidence, BootstrapJobError> {
    if expectations.len() != acquired.len() {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    let count = templates
        .len()
        .checked_add(acquired.len())
        .ok_or(BootstrapJobError::SemanticEvidence)?;
    if count > amiss_wire::requests::SEMANTIC_EVIDENCE_REQUEST_LIMIT {
        return Err(BootstrapJobError::SemanticEvidence);
    }

    let mut expected = normalized_expectations(expectations)?
        .into_iter()
        .map(|expectation| (expectation.acquisition_identity.clone(), expectation))
        .collect::<BTreeMap<_, _>>();
    let mut bound = Vec::with_capacity(count);
    for template in templates {
        let template_bytes = amiss_wire::semantic::template(template.clone())
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
        bound.push(bind_input(
            template,
            None,
            template_bytes.into(),
            candidate_identity_digest,
        )?);
    }
    for source in acquired {
        let template = amiss_wire::semantic::parse_template(&source.bytes)
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
        let actual = SemanticEvidenceExpectation {
            acquisition_identity: source.acquisition_identity.clone(),
            producer_kind: template.producer.kind,
            producer_identity: template.producer.identity.clone(),
            producer_version: template.producer.version.clone(),
            context_digest: template.producer.context_digest,
        };
        if expected.remove(&source.acquisition_identity).as_ref() != Some(&actual) {
            return Err(BootstrapJobError::SemanticEvidence);
        }
        bound.push(bind_input(
            &template,
            Some(source.acquisition_identity.clone()),
            Arc::clone(&source.bytes),
            candidate_identity_digest,
        )?);
    }

    bound.sort_by_key(|input| input.payload_digest);
    if !expected.is_empty()
        || bound.windows(2).any(
            |pair| matches!(pair, [left, right] if left.payload_digest == right.payload_digest),
        )
    {
        return Err(BootstrapJobError::SemanticEvidence);
    }
    let artifact = if bound.is_empty() {
        None
    } else {
        Some(input_artifact(&bound, SEMANTIC_INPUT_ARTIFACT_BYTES)?)
    };
    Ok(BoundSemanticEvidence {
        supplied: bound.into_iter().map(|input| input.supplied).collect(),
        artifact,
    })
}

fn bind_input(
    template: &SemanticEvidenceTemplate,
    acquisition_identity: Option<ArtifactId>,
    template_bytes: Arc<[u8]>,
    candidate_identity_digest: Digest,
) -> Result<BoundInput, BootstrapJobError> {
    let (envelope, envelope_bytes) =
        amiss_wire::semantic::bind_template(template, candidate_identity_digest)
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    Ok(BoundInput {
        payload_digest: envelope.payload_digest,
        supplied: SuppliedSemanticEvidence {
            value: serde_json::from_slice(&envelope_bytes)
                .map_err(|_defect| BootstrapJobError::SemanticEvidence)?,
            expected_context_digest: template.producer.context_digest,
        },
        acquisition_identity,
        template_digest: sha256(&template_bytes),
        template_bytes,
        envelope_digest: sha256(&envelope_bytes),
        envelope_bytes,
    })
}

fn input_artifact(inputs: &[BoundInput], limit: u64) -> Result<Vec<u8>, BootstrapJobError> {
    let mut artifact = InputArtifact {
        inputs: inputs
            .iter()
            .map(|input| InputArtifactRow {
                acquisition_identity: input.acquisition_identity.as_ref(),
                envelope_bytes_base64: String::new(),
                envelope_digest: input.envelope_digest,
                payload_digest: input.payload_digest,
                template_bytes_base64: String::new(),
                template_digest: input.template_digest,
            })
            .collect(),
        schema: InputArtifactSchema::Current,
    };
    let metadata =
        serde_json::to_vec(&artifact).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let mut projected_length =
        u64::try_from(metadata.len()).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    for input in inputs {
        let template_length = base64::encoded_len(input.template_bytes.len(), true)
            .and_then(|length| u64::try_from(length).ok())
            .ok_or(BootstrapJobError::SemanticEvidence)?;
        let envelope_length = base64::encoded_len(input.envelope_bytes.len(), true)
            .and_then(|length| u64::try_from(length).ok())
            .ok_or(BootstrapJobError::SemanticEvidence)?;
        projected_length = projected_length
            .checked_add(template_length)
            .and_then(|length| length.checked_add(envelope_length))
            .filter(|length| *length <= limit)
            .ok_or(BootstrapJobError::SemanticEvidence)?;
    }
    for (row, input) in artifact.inputs.iter_mut().zip(inputs) {
        row.template_bytes_base64 =
            base64::engine::general_purpose::STANDARD.encode(&input.template_bytes);
        row.envelope_bytes_base64 =
            base64::engine::general_purpose::STANDARD.encode(&input.envelope_bytes);
    }
    let bytes =
        serde_json::to_vec(&artifact).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    (u64::try_from(bytes.len()).ok() == Some(projected_length))
        .then_some(bytes)
        .ok_or(BootstrapJobError::SemanticEvidence)
}
