use amiss_wire::digest::Digest;
use amiss_wire::model::ArtifactId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputArtifact<I = ArtifactId> {
    pub(crate) inputs: Vec<InputArtifactRow<I>>,
    pub(crate) schema: InputArtifactSchema,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputArtifactRow<I = ArtifactId> {
    pub(crate) acquisition_identity: Option<I>,
    pub(crate) envelope_bytes_base64: String,
    pub(crate) envelope_digest: Digest,
    pub(crate) payload_digest: Digest,
    pub(crate) template_bytes_base64: String,
    pub(crate) template_digest: Digest,
}

#[derive(
    serde_with::DeserializeFromStr, serde_with::SerializeDisplay, strum::EnumString, strum::Display,
)]
pub(crate) enum InputArtifactSchema {
    #[strum(serialize = "amiss/controller-semantic-input-artifact-v1")]
    Current,
}
