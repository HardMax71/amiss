use std::borrow::Cow;

use amiss_wire::digest::Digest;
use amiss_wire::model::ArtifactId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputArtifact<'a, I = ArtifactId> {
    pub inputs: Vec<InputArtifactRow<'a, I>>,
    pub schema: InputArtifactSchema,
}

#[serde_with::serde_as]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputArtifactRow<'a, I = ArtifactId> {
    pub acquisition_identity: Option<I>,
    #[serde_as(as = "serde_with::base64::Base64")]
    #[serde(rename = "envelope_bytes_base64")]
    pub envelope_bytes: Cow<'a, [u8]>,
    pub envelope_digest: Digest,
    pub payload_digest: Digest,
    #[serde_as(as = "serde_with::base64::Base64")]
    #[serde(rename = "template_bytes_base64")]
    pub template_bytes: Cow<'a, [u8]>,
    pub template_digest: Digest,
}

#[derive(
    serde_with::DeserializeFromStr, serde_with::SerializeDisplay, strum::EnumString, strum::Display,
)]
pub enum InputArtifactSchema {
    #[strum(serialize = "amiss/controller-semantic-input-artifact-v1")]
    Current,
}
