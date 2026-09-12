use js_int::UInt;
use json_serde::deserialize_some;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BranchRule {
    RequiredStatusChecks(RequiredStatusRule),
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredStatusRule {
    pub parameters: RequiredStatusParameters,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredStatusParameters {
    pub required_status_checks: Vec<RequiredStatus>,
    pub strict_required_status_checks_policy: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredStatus {
    pub context: String,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub integration_id: Option<UInt>,
}
