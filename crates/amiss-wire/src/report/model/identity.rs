use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::IgnoredAny};

use crate::requests::CandidateIdentitySchema;

#[derive(Deserialize)]
pub struct IdentityPayload<E = IdentityEvaluation> {
    pub evaluation: E,
}

#[derive(Deserialize, Serialize)]
pub struct IdentityEvaluation {
    #[serde(default, skip_serializing)]
    pub schema: json_serde::Absent,
    #[serde(default, skip_serializing)]
    pub evaluation_instant: IgnoredAny,
    #[serde(default, skip_serializing)]
    pub trusted_time: IgnoredAny,
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize)]
pub struct IdentityPreimage {
    #[serde(flatten)]
    pub evaluation: IdentityEvaluation,
    pub schema: CandidateIdentitySchema,
}
