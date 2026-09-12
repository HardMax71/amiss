use serde::{Deserialize, Serialize};
use serde_with::{As, DisplayFromStr, OneOrMany, PickFirst, Same};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
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
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct JobConfig {
    pub url: String,
    pub sha: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestHint {
    pub merge_request_iid: u64,
}
