use std::sync::Arc;

use amiss_controller::{BootstrapJobError, SemanticEvidenceTemplate, bind_semantic_evidence};
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
                kind: amiss_wire::semantic::SemanticProducerKind::RecordSet,
                identity: ArtifactId::new("test-records".to_owned())
                    .ok_or(BootstrapJobError::SemanticEvidence)?,
                version: "1".to_owned(),
                context_digest: amiss_wire::model::Digest::from([
                    0x7e, 0x23, 0xf6, 0xee, 0x04, 0xbb, 0x34, 0xdd, 0x84, 0xac, 0xb6, 0xd4, 0x61,
                    0x7b, 0x11, 0x64, 0xe6, 0xa1, 0x3f, 0xc3, 0x46, 0xba, 0x3e, 0x46, 0xa3, 0xad,
                    0x7c, 0xf7, 0x1c, 0x68, 0x21, 0x54,
                ]),
                input_digest: amiss_wire::model::Digest::from([
                    0x57, 0xf9, 0x0d, 0x23, 0x2c, 0x39, 0x61, 0xd7, 0x7e, 0x59, 0xb8, 0xfb, 0x9b,
                    0xee, 0x47, 0xff, 0xb9, 0x2e, 0x47, 0x0e, 0xa4, 0x01, 0x7d, 0xeb, 0x62, 0xea,
                    0x25, 0x78, 0x7e, 0x5c, 0xca, 0x30,
                ]),
            },
            complete: true,
            observations: Arc::from([]),
        }],
        &[],
        &[],
        amiss_wire::model::Digest::from([
            0x9c, 0x12, 0x31, 0xe6, 0x05, 0x14, 0x56, 0xcc, 0xd0, 0x1c, 0x1b, 0xc9, 0x01, 0x8b,
            0x72, 0x7b, 0xd4, 0x3b, 0xdb, 0x29, 0x48, 0xb3, 0xda, 0xca, 0xab, 0x91, 0x5c, 0x4f,
            0xf0, 0x2d, 0xcc, 0x32,
        ]),
    )?;
    let payload_digests = bound
        .supplied
        .iter()
        .map(|supplied| supplied.value.payload_digest)
        .collect::<Vec<_>>();
    Ok(SemanticInputArtifact {
        report: amiss_fixtures::semantic_report(&payload_digests)
            .ok_or(BootstrapJobError::SemanticEvidence)?,
        artifact: bound.artifact.ok_or(BootstrapJobError::SemanticEvidence)?,
    })
}
