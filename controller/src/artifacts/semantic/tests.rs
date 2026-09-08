#![cfg(test)]

use std::sync::Arc;

use amiss_wire::digest::{hb, sha256};
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{SemanticProducer, TemplateSchema};

use super::validate;
use crate::{ArtifactError, SemanticEvidenceTemplate, bind_semantic_evidence};

#[test]
fn exact_inputs_bind_to_the_report_and_every_byte_is_replayable() -> Result<(), ArtifactError> {
    let candidate = hb("amiss/test-candidate", b"candidate");
    let template: SemanticEvidenceTemplate<'static> = SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: SemanticProducer {
            kind: amiss_wire::semantic::SemanticProducerKind::RecordSet,
            identity: ArtifactId::new("test-records".to_owned()).ok_or(ArtifactError::Corrupt)?,
            version: "1".to_owned(),
            context_digest: hb("amiss/test-context", b"context"),
            input_digest: hb("amiss/test-input", b"input"),
        },
        complete: true,
        observations: Arc::from([]),
    };
    let mut bound = bind_semantic_evidence(&[template], &[], &[], candidate)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let artifact = bound.artifact.as_mut().ok_or(ArtifactError::Corrupt)?;
    let row = &mut artifact.inputs[0];
    let mut template_bytes = row.template_bytes.to_vec();
    template_bytes.push(b'\n');
    row.template_digest = sha256(&template_bytes);
    row.template_bytes = template_bytes.into();
    let payload_digest = row.payload_digest;
    let report = amiss_fixtures::captured_report(
        amiss_fixtures::semantic_report(&[payload_digest]).ok_or(ArtifactError::Corrupt)?,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;

    validate(&report.envelope, &bound)?;
    let other_report = amiss_fixtures::captured_report(
        amiss_fixtures::semantic_report(&[hb("amiss/test-other", b"other")])
            .ok_or(ArtifactError::Corrupt)?,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    assert!(matches!(
        validate(&other_report.envelope, &bound),
        Err(ArtifactError::Corrupt)
    ));

    let mut tampered = bound;
    tampered
        .artifact
        .as_mut()
        .ok_or(ArtifactError::Corrupt)?
        .inputs[0]
        .template_digest = hb("amiss/test-other", b"other");
    assert!(matches!(
        validate(&report.envelope, &tampered),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}
