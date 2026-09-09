use serde::{Deserialize, Serialize};
use serde_with::{As, DisplayFromStr, OneOrMany, PickFirst, Same};

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    #[serde(with = "As::<OneOrMany<Same>>")]
    pub aud: Vec<String>,
    pub exp: u64,
    pub nbf: u64,
    pub iat: u64,
    pub jti: String,
    #[serde(with = "As::<PickFirst<(Same, DisplayFromStr)>>")]
    pub job_project_id: u64,
    pub job_project_path: String,
    #[serde(with = "As::<PickFirst<(Same, DisplayFromStr)>>")]
    pub pipeline_id: u64,
    pub pipeline_source: String,
    #[serde(with = "As::<PickFirst<(Same, DisplayFromStr)>>")]
    pub job_id: u64,
    #[serde(with = "As::<PickFirst<(Same, DisplayFromStr)>>")]
    pub runner_id: u64,
    pub runner_environment: String,
    pub sha: String,
    pub job_source: String,
    pub job_config: JobConfig,
    #[serde(with = "As::<PickFirst<(Same, DisplayFromStr)>>")]
    pub project_id: u64,
    pub project_path: String,
    #[serde(with = "As::<PickFirst<(Same, DisplayFromStr)>>")]
    pub namespace_id: u64,
    pub namespace_path: String,
    #[serde(with = "As::<PickFirst<(Same, DisplayFromStr)>>")]
    pub job_namespace_id: u64,
    pub job_namespace_path: String,
    pub user_id: String,
    #[serde(deserialize_with = "Option::deserialize")]
    #[serialize_always]
    pub user_login: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    #[serialize_always]
    pub user_email: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    #[serialize_always]
    pub user_access_level: Option<String>,
    #[serde(rename = "ref")]
    pub branch: String,
    pub ref_type: String,
    pub ref_path: String,
    pub ref_protected: Protection,
    #[serde(deserialize_with = "Option::deserialize")]
    #[serialize_always]
    pub ci_config_ref_uri: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    #[serialize_always]
    pub ci_config_sha: Option<String>,
    pub project_visibility: String,
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub target_audience: Option<String>,
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub user_identities: Option<Vec<UserIdentity>>,
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub groups_direct: Option<Vec<String>>,
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub environment: Option<String>,
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub environment_protected: Option<Protection>,
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub deployment_tier: Option<String>,
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub environment_action: Option<String>,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    serde_with::DeserializeFromStr,
    serde_with::SerializeDisplay,
)]
pub enum Protection {
    #[strum(serialize = "true")]
    Protected,
    #[strum(serialize = "false")]
    Unprotected,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobConfig {
    pub url: String,
    pub sha: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UserIdentity {
    pub provider: String,
    pub extern_uid: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestHint {
    pub merge_request_iid: u64,
}
