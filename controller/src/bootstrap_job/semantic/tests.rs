#![cfg(test)]

use std::sync::Arc;

use amiss_wire::digest::hb;
use amiss_wire::json;
use amiss_wire::model::ArtifactId;

use super::{bind_input, input_artifact};
use crate::{BootstrapJobError, SemanticEvidenceTemplate};

#[test]
fn an_input_artifact_admits_its_exact_size_and_refuses_the_next_lower_limit()
-> Result<(), BootstrapJobError> {
    let template = SemanticEvidenceTemplate {
        producer_kind: ArtifactId::new("record-set".to_owned())
            .ok_or(BootstrapJobError::SemanticEvidence)?,
        producer_identity: ArtifactId::new("test-records".to_owned())
            .ok_or(BootstrapJobError::SemanticEvidence)?,
        producer_version: "1".to_owned(),
        context_digest: hb("amiss/test-context", b"context"),
        input_digest: hb("amiss/test-input", b"input"),
        complete: true,
        observations: Arc::from([]),
    };
    let value = amiss_wire::semantic::template(template.clone())
        .map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let input = bind_input(
        &template,
        None,
        Arc::from(json::canonical(&value)),
        hb("amiss/test-candidate", b"candidate"),
    )?;
    let artifact = input_artifact(std::slice::from_ref(&input), u64::MAX)?;
    let exact =
        u64::try_from(artifact.len()).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;
    let parsed = json::parse(&artifact).map_err(|_defect| BootstrapJobError::SemanticEvidence)?;

    assert_eq!(json::canonical(&parsed), artifact);
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
