use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::digest::Digest;

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AssessmentVerdict {
    Matched,
    Refuted,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Nullable<T> {
    Value(T),
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentEngine {
    pub engine_version: String,
    pub engine_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentSubject {
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    pub evidence_payload_digest: Nullable<Digest>,
}
