use std::sync::Arc;

use amiss_wire::digest::Digest;
use amiss_wire::model::ArtifactId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputArtifact<I = ArtifactId> {
    pub inputs: Vec<InputArtifactRow<I>>,
    pub schema: InputArtifactSchema,
}

#[serde_with::serde_as]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputArtifactRow<I = ArtifactId> {
    pub acquisition_identity: Option<I>,
    #[serde_as(as = "serde_with::base64::Base64")]
    #[serde(rename = "envelope_bytes_base64")]
    pub envelope_bytes: Arc<[u8]>,
    pub envelope_digest: Digest,
    pub payload_digest: Digest,
    #[serde_as(as = "serde_with::base64::Base64")]
    #[serde(rename = "template_bytes_base64")]
    pub template_bytes: Arc<[u8]>,
    pub template_digest: Digest,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde_with::DeserializeFromStr,
    serde_with::SerializeDisplay,
    strum::EnumString,
    strum::Display,
)]
pub enum InputArtifactSchema {
    #[strum(serialize = "amiss/controller-semantic-input-artifact-v1")]
    Current,
}

/// Checks encoded size before allocating base64 output.
///
/// # Errors
/// Returns an error if metadata cannot be encoded or the artifact exceeds the ceiling.
pub fn input_artifact_size(
    artifact: &InputArtifact,
    limit: u64,
) -> Result<u64, amiss_wire::JsonInputError> {
    let empty: Arc<[u8]> = Arc::from([]);
    let metadata = InputArtifact {
        schema: artifact.schema,
        inputs: artifact
            .inputs
            .iter()
            .map(|input| InputArtifactRow {
                acquisition_identity: input.acquisition_identity.as_ref(),
                envelope_bytes: Arc::clone(&empty),
                envelope_digest: input.envelope_digest,
                payload_digest: input.payload_digest,
                template_bytes: Arc::clone(&empty),
                template_digest: input.template_digest,
            })
            .collect(),
    };
    let mut counter = countio::Counter::new(std::io::sink());
    serde_json::to_writer(&mut counter, &metadata)?;
    let metadata_size = u64::try_from(counter.writer_bytes())
        .ok()
        .filter(|size| *size <= limit)
        .ok_or(amiss_wire::JsonInputError::LimitExceeded)?;
    artifact
        .inputs
        .iter()
        .flat_map(|row| [&row.template_bytes, &row.envelope_bytes])
        .try_fold(metadata_size, |size, bytes| {
            base64::encoded_len(bytes.len(), true)
                .and_then(|length| u64::try_from(length).ok())
                .and_then(|length| size.checked_add(length))
                .filter(|size| *size <= limit)
                .ok_or(amiss_wire::JsonInputError::LimitExceeded)
        })
}
