#![cfg(test)]

use amiss_wire::model::{ArtifactId, Digest};
use amiss_wire::semantic::{SemanticProducer, TemplateSchema};
use base64::Engine as _;
use sha2::Digest as _;
use std::sync::Arc;

use super::validate;
use crate::semantic_artifact::{InputArtifact, InputArtifactRow, InputArtifactSchema};
use crate::{ArtifactError, SemanticEvidenceTemplate};

#[test]
fn exact_inputs_bind_to_the_report_and_every_byte_is_replayable() -> Result<(), ArtifactError> {
    let candidate = Digest::from([1; 32]);
    let other = Digest::from([4; 32]);
    let template: SemanticEvidenceTemplate<'static> = SemanticEvidenceTemplate {
        schema: TemplateSchema::Current,
        producer: SemanticProducer {
            kind: amiss_wire::semantic::SemanticProducerKind::RecordSet,
            identity: ArtifactId::new("test-records".to_owned()).ok_or(ArtifactError::Corrupt)?,
            version: "1".to_owned(),
            context_digest: Digest::from([2; 32]),
            input_digest: Digest::from([3; 32]),
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
    amiss_wire::semantic::write(&envelope, &mut envelope_bytes)
        .map_err(|_defect| ArtifactError::Corrupt)?;
    let payload_digest = envelope.payload_digest;
    let artifact = serde_json::to_vec(&InputArtifact {
        inputs: vec![InputArtifactRow {
            acquisition_identity: Some(
                ArtifactId::new("test-artifact".to_owned()).ok_or(ArtifactError::Corrupt)?,
            ),
            envelope_bytes_base64: base64::engine::general_purpose::STANDARD
                .encode(&envelope_bytes),
            envelope_digest: Digest::from(sha2::Sha256::digest(&envelope_bytes).0),
            payload_digest,
            template_bytes_base64: base64::engine::general_purpose::STANDARD
                .encode(&template_bytes),
            template_digest: Digest::from(sha2::Sha256::digest(&template_bytes).0),
        }],
        schema: InputArtifactSchema::Current,
    })
    .map_err(|_defect| ArtifactError::Corrupt)?;
    let report =
        amiss_fixtures::semantic_report(&[payload_digest]).ok_or(ArtifactError::Corrupt)?;

    validate(&report, &artifact)?;
    assert!(matches!(
        validate(
            &amiss_fixtures::semantic_report(&[other]).ok_or(ArtifactError::Corrupt)?,
            &artifact
        ),
        Err(ArtifactError::Corrupt)
    ));

    for (path, value) in [
        ("/schema", serde_json::json!("another-artifact")),
        (
            "/schema",
            serde_json::json!({ "amiss/controller-semantic-input-artifact-v1": null }),
        ),
        (
            "/inputs/0/acquisition_identity",
            serde_json::json!("../bad"),
        ),
        ("/inputs/0/template_digest", serde_json::json!(other)),
        ("/inputs/0/envelope_digest", serde_json::json!("SHA256:bad")),
        ("/inputs/0/payload_digest", serde_json::json!(null)),
        ("/inputs/0/template_bytes_base64", serde_json::json!("A")),
        (
            "/inputs/0/envelope_bytes_base64",
            serde_json::json!("not base64"),
        ),
    ] {
        let mut tampered: serde_json::Value =
            serde_json::from_slice(&artifact).map_err(|_defect| ArtifactError::Corrupt)?;
        *tampered.pointer_mut(path).ok_or(ArtifactError::Corrupt)? = value;
        let tampered = serde_json::to_vec(&tampered).map_err(|_defect| ArtifactError::Corrupt)?;
        assert!(
            matches!(validate(&report, &tampered), Err(ArtifactError::Corrupt)),
            "{path}"
        );
    }
    Ok(())
}
