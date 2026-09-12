#![cfg(test)]

use sha2::Digest as _;
use std::sync::Arc;

use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{SemanticProducer, TemplateSchema};

use super::{bind_input, input_artifact};
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
            context_digest: amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix("amiss/test-context")
                    .chain_update([0_u8])
                    .chain_update(b"context")
                    .finalize()
                    .0,
            ),
            input_digest: amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix("amiss/test-input")
                    .chain_update([0_u8])
                    .chain_update(b"input")
                    .finalize()
                    .0,
            ),
        },
        complete: true,
        observations: Arc::from([]),
    };
    let template_bytes = amiss_wire::semantic::template(template.clone())
        .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let input = bind_input(
        &template,
        None,
        template_bytes.into(),
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/test-candidate")
                .chain_update([0_u8])
                .chain_update(b"candidate")
                .finalize()
                .0,
        ),
    )?;
    let artifact = input_artifact(std::slice::from_ref(&input), u64::MAX)?;
    let exact =
        u64::try_from(artifact.len()).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let parsed = serde_json::from_slice::<serde_json::Value>(&artifact)
        .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;

    assert_eq!(serde_json_canonicalizer::to_vec(&parsed).unwrap(), artifact);
    assert_eq!(
        input_artifact(std::slice::from_ref(&input), exact)?,
        artifact
    );
    assert_eq!(
        input_artifact(std::slice::from_ref(&input), exact.saturating_sub(1)),
        Err(BootstrapJobError::SemanticEvidence)
    );
    Ok(())
}
