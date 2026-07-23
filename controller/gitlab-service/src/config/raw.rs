use std::path::PathBuf;

use amiss_controller_service::{CheckPlanFiles, ExecutionLimits, ExecutionPaths};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConfig {
    pub(super) listen: String,
    pub(super) evaluation_path: String,
    pub(super) max_concurrent_evaluations: usize,
    pub(super) gitlab: RawGitLab,
    pub(super) policy: RawPolicy,
    pub(super) plan: CheckPlanFiles,
    pub(super) paths: ExecutionPaths,
    #[serde(default)]
    pub(super) limits: ExecutionLimits,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawGitLab {
    pub(super) instance: String,
    pub(super) api_base: String,
    pub(super) api_token_file: PathBuf,
    pub(super) git: RawGit,
    pub(super) oidc: RawOidc,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawGit {
    pub(super) username: String,
    pub(super) token_file: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawOidc {
    pub(super) issuer: String,
    pub(super) audience: String,
    pub(super) trust_set: String,
    pub(super) keys: Vec<RawOidcKey>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawOidcKey {
    pub(super) kid: String,
    pub(super) anchor: String,
    pub(super) public_key_file: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPolicy {
    pub(super) integration: String,
    pub(super) project_id: u64,
    pub(super) project_path: String,
    pub(super) target_branch: String,
    pub(super) job_name: String,
    pub(super) config_url: String,
    pub(super) config_commit: String,
    pub(super) gitlab_hosted_runners: bool,
    #[serde(default)]
    pub(super) self_hosted_runner_ids: Vec<u64>,
}
