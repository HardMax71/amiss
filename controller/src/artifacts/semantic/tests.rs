#![cfg(test)]

use std::sync::Arc;

use amiss_wire::digest::{hb, sha256};
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{SemanticProducer, TemplateSchema};

use super::validate;
use crate::semantic_artifact::{InputArtifact, InputArtifactRow, InputArtifactSchema};
use crate::{ArtifactError, SemanticEvidenceTemplate};

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
    let mut template_bytes = amiss_wire::semantic::template(template.clone())
        .map_err(|_defect| ArtifactError::Corrupt)?;
    template_bytes.push(b'\n');
    let envelope = amiss_wire::semantic::bind_template(&template, candidate)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let mut envelope_bytes = Vec::new();
    amiss_wire::write_json(
        &envelope,
        &mut envelope_bytes,
        amiss_wire::semantic::SEMANTIC_EVIDENCE_BYTES,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let payload_digest = envelope.payload_digest;
    let artifact = InputArtifact {
        inputs: vec![InputArtifactRow {
            acquisition_identity: Some(
                ArtifactId::new("test-artifact".to_owned()).ok_or(ArtifactError::Corrupt)?,
            ),
            envelope_bytes: envelope_bytes.as_slice().into(),
            envelope_digest: sha256(&envelope_bytes),
            payload_digest,
            template_bytes: template_bytes.as_slice().into(),
            template_digest: sha256(&template_bytes),
        }],
        schema: InputArtifactSchema::Current,
    };
    let report = amiss_fixtures::captured_report(
        amiss_fixtures::semantic_report(&[payload_digest]).ok_or(ArtifactError::Corrupt)?,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;

    validate(&report.envelope, &artifact)?;
    let other_report = amiss_fixtures::captured_report(
        amiss_fixtures::semantic_report(&[hb("amiss/test-other", b"other")])
            .ok_or(ArtifactError::Corrupt)?,
    )
    .map_err(|_defect| ArtifactError::Corrupt)?;
    assert!(matches!(
        validate(&other_report.envelope, &artifact),
        Err(ArtifactError::Corrupt)
    ));

    let mut tampered = artifact;
    tampered.inputs[0].template_digest = hb("amiss/test-other", b"other");
    assert!(matches!(
        validate(&report.envelope, &tampered),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}
