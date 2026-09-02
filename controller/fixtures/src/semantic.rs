use std::sync::Arc;

use amiss_controller::{BootstrapJobError, SemanticEvidenceTemplate, bind_semantic_evidence};
use amiss_wire::digest::hb;
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{SemanticProducer, TemplateSchema};

pub struct SemanticInputArtifact {
    pub report: Vec<u8>,
    pub artifact: Vec<u8>,
}

/// Builds one report-bound semantic-input audit artifact.
///
/// # Errors
///
/// The fixed template cannot be bound or decoded under the production contract.
pub fn semantic_input_artifact() -> Result<SemanticInputArtifact, BootstrapJobError> {
    let bound = bind_semantic_evidence(
        &[SemanticEvidenceTemplate {
            schema: TemplateSchema::Current,
            producer: SemanticProducer {
                kind: ArtifactId::new("record-set".to_owned())
                    .ok_or(BootstrapJobError::SemanticEvidence)?,
                identity: ArtifactId::new("test-records".to_owned())
                    .ok_or(BootstrapJobError::SemanticEvidence)?,
                version: "1".to_owned(),
                context_digest: hb("amiss/test-context", b"context"),
                input_digest: hb("amiss/test-input", b"input"),
            },
            complete: true,
            observations: Arc::from([]),
        }],
        &[],
        &[],
        hb("amiss/test-candidate", b"candidate"),
    )?;
    let payload_digests = bound
        .supplied
        .iter()
        .map(|supplied| {
            let bytes = serde_json::to_vec(&supplied.value)
                .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
            amiss_wire::semantic::parse(&bytes)
                .map(|envelope| envelope.payload_digest)
                .map_err(|_defect| BootstrapJobError::SemanticEvidence)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SemanticInputArtifact {
        report: amiss_fixtures::semantic_report(&payload_digests),
        artifact: bound.artifact.ok_or(BootstrapJobError::SemanticEvidence)?,
    })
}
