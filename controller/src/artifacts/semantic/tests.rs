#![cfg(test)]

use std::sync::Arc;

use amiss_wire::digest::{hb, sha256};
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::{SemanticProducer, TemplateSchema};
use base64::Engine as _;

use super::{INPUT_ARTIFACT_SCHEMA, InputArtifact, InputArtifactRow, validate};
use crate::{ArtifactError, SemanticEvidenceTemplate};

#[test]
fn exact_inputs_bind_to_the_report_and_every_byte_is_replayable() -> Result<(), ArtifactError> {
    let candidate = hb("amiss/test-candidate", b"candidate");
    let template: SemanticEvidenceTemplate = SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: SemanticProducer {
            kind: ArtifactId::new("record-set".to_owned()).ok_or(ArtifactError::Corrupt)?,
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
    let envelope_bytes = amiss_wire::semantic::bind_template(&template, candidate)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let payload_digest = amiss_wire::semantic::parse(&envelope_bytes)
        .map_err(|_defect| ArtifactError::Corrupt)?
        .payload_digest;
    let artifact = serde_json::to_vec(&InputArtifact {
        inputs: vec![InputArtifactRow {
            acquisition_identity: Some("test-artifact".to_owned()),
            envelope_bytes_base64: base64::engine::general_purpose::STANDARD
                .encode(&envelope_bytes),
            envelope_digest: sha256(&envelope_bytes).to_string(),
            payload_digest: payload_digest.to_string(),
            template_bytes_base64: base64::engine::general_purpose::STANDARD
                .encode(&template_bytes),
            template_digest: sha256(&template_bytes).to_string(),
        }],
        schema: INPUT_ARTIFACT_SCHEMA.to_owned(),
    })
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let report =
        amiss_fixtures::semantic_report(&[payload_digest]).ok_or(ArtifactError::Corrupt)?;

    validate(&report, &artifact)?;
    assert!(matches!(
        validate(
            &amiss_fixtures::semantic_report(&[hb("amiss/test-other", b"other")])
                .ok_or(ArtifactError::Corrupt)?,
            &artifact
        ),
        Err(ArtifactError::Corrupt)
    ));

    let mut tampered: serde_json::Value =
        serde_json::from_slice(&artifact).map_err(|_defect| ArtifactError::Corrupt)?;
    tampered["inputs"][0]["template_digest"] =
        serde_json::Value::String(hb("amiss/test-other", b"other").to_string());
    let tampered = serde_json::to_vec(&tampered).map_err(|_defect| ArtifactError::Corrupt)?;
    assert!(matches!(
        validate(&report, &tampered),
        Err(ArtifactError::Corrupt)
    ));
    Ok(())
}
