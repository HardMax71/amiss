mod tests;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::digest::{Digest, sha256};
use amiss_wire::model::ArtifactId;
use amiss_wire::requests::SuppliedSemanticEvidence;
use amiss_wire::semantic::{SemanticEvidence, SemanticEvidenceEnvelope};

use super::plan::normalized_expectations;
use super::{
    AcquiredSemanticTemplate, BootstrapJobError, BoundSemanticEvidence,
    SEMANTIC_INPUT_ARTIFACT_BYTES, SemanticEvidenceExpectation, SemanticEvidenceTemplate,
};
use crate::semantic_artifact::{
    InputArtifact, InputArtifactRow, InputArtifactSchema, input_artifact_size,
};

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
    templates: &[SemanticEvidenceTemplate<'_>],
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
    let (supplied, inputs): (Vec<_>, Vec<_>) = bound
        .into_iter()
        .map(|input| {
            (
                input.supplied,
                InputArtifactRow {
                    acquisition_identity: input.acquisition_identity,
                    envelope_bytes: input.envelope_bytes.into(),
                    envelope_digest: input.envelope_digest,
                    payload_digest: input.payload_digest,
                    template_bytes: input.template_bytes,
                    template_digest: input.template_digest,
                },
            )
        })
        .unzip();
    let artifact = if inputs.is_empty() {
        None
    } else {
        let artifact = InputArtifact {
            inputs,
            schema: InputArtifactSchema::Current,
        };
        input_artifact_size(&artifact, SEMANTIC_INPUT_ARTIFACT_BYTES)
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
        Some(Arc::new(artifact))
    };
    Ok(BoundSemanticEvidence { supplied, artifact })
}

fn bind_input(
    template: &SemanticEvidenceTemplate<'_>,
    acquisition_identity: Option<ArtifactId>,
    template_bytes: Arc<[u8]>,
    candidate_identity_digest: Digest,
) -> Result<BoundInput, BootstrapJobError> {
    let envelope = amiss_wire::semantic::bind_template(template, candidate_identity_digest)
        .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let mut envelope_bytes = Vec::new();
    amiss_wire::write_json(
        &envelope,
        &mut envelope_bytes,
        amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES,
    )
    .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    Ok(BoundInput {
        payload_digest: envelope.payload_digest,
        supplied: SuppliedSemanticEvidence {
            value: Arc::new(SemanticEvidenceEnvelope {
                payload: SemanticEvidence {
                    observations: envelope
                        .payload
                        .observations
                        .into_iter()
                        .map(|row| Cow::Owned(row.into_owned()))
                        .collect(),
                    ..envelope.payload
                },
                ..envelope
            }),
            expected_context_digest: template.producer.context_digest,
        },
        acquisition_identity,
        template_digest: sha256(&template_bytes),
        template_bytes,
        envelope_digest: sha256(&envelope_bytes),
        envelope_bytes,
    })
}
