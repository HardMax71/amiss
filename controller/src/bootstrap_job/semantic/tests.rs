#![cfg(test)]

use std::sync::Arc;

use amiss_wire::digest::hb;
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{SemanticProducer, TemplateSchema};

use super::bind_semantic_evidence;
use crate::semantic_artifact::{InputArtifact, input_artifact_size};
use crate::{BootstrapJobError, SemanticEvidenceTemplate};

#[test]
fn an_input_artifact_admits_its_exact_size_and_refuses_the_next_lower_limit()
-> Result<(), BootstrapJobError> {
    let template = SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: SemanticProducer {
            kind: amiss_wire::semantic::SemanticProducerKind::RecordSet,
            identity: ArtifactId::new("test-records".to_owned())
                .ok_or(BootstrapJobError::SemanticEvidence)?,
            version: "1".to_owned(),
            context_digest: hb("amiss/test-context", b"context"),
            input_digest: hb("amiss/test-input", b"input"),
        },
        complete: true,
        observations: Arc::from([]),
    };
    let bound = bind_semantic_evidence(
        &[template],
        &[],
        &[],
        hb("amiss/test-candidate", b"candidate"),
    )?;
    let artifact = bound.artifact.ok_or(BootstrapJobError::SemanticEvidence)?;
    let bytes =
        serde_json::to_vec(&artifact).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let exact =
        u64::try_from(bytes.len()).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let parsed: InputArtifact = amiss_wire::read_json(&bytes, exact)
        .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;

    assert_eq!(&parsed, artifact.as_ref());
    assert_eq!(serde_json_canonicalizer::to_vec(&parsed).unwrap(), bytes);
    assert_eq!(
        input_artifact_size(&artifact, exact)
            .map_err(|_defect| BootstrapJobError::SemanticEvidence)?,
        exact
    );
    assert!(matches!(
        input_artifact_size(&artifact, exact.saturating_sub(1)),
        Err(amiss_wire::JsonInputError::LimitExceeded)
    ));
    Ok(())
}
