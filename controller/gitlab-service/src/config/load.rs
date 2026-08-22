mod tests;

use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{DeliveryRoute, FileLedgerConfig, SignedTimePolicy, TrustSetId};
use amiss_controller_git::GitFetchBounds;
use amiss_controller_gitlab::{GitLabClient, GitLabOidc, GitLabTimeouts};
use amiss_controller_service::{
    ConfigError, load_artifact_service, load_execution_limits, load_execution_paths, load_plan,
    read_regular,
};
use amiss_wire::model::ObjectFormat;
use secrecy::SecretString;

use crate::objects::GitLabGitObjects;

use super::ServiceConfig;
use super::identity::{keys, policy, provider, scope};
use super::raw::RawConfig;

const TOKEN_BYTES: u64 = 4_096;
const PROVIDER_RESPONSE_BYTES: usize = 4 * 1_024 * 1_024;
const HINT_BODY_BYTES: usize = 1_024;
const POLICY_JOB_HEADERS: u64 = 32;
const POLICY_JOB_HEADER_BYTES: u64 = 32 * 1_024;

pub(super) fn load(raw: RawConfig) -> Result<ServiceConfig, ConfigError> {
    let listen = raw
        .listen
        .parse()
        .map_err(|defect| ConfigError::caused_by("listen must be one socket address", defect))?;
    let provider = provider(raw.gitlab.instance)?;
    let policy = policy(raw.policy)?;
    let plan = Arc::new(load_plan(&raw.plan)?);
    validate_action(&provider, &plan)?;
    let scope = scope(&provider, &policy)?;
    let limits = load_execution_limits(
        &raw.limits,
        raw.evaluation_path,
        raw.max_concurrent_evaluations,
    )?;
    let evaluation = policy_job_endpoint(limits.evaluation);
    let paths = load_execution_paths(&raw.paths, &plan)?;
    let artifacts = load_artifact_service(
        &raw.artifacts,
        paths.artifacts.clone(),
        limits.artifacts,
        &evaluation,
    )?;
    let api_token = load_token(&raw.gitlab.api_token_file)?;
    let git_token = load_token(&raw.gitlab.git.token_file)?;
    let repository_url = format!(
        "https://{}/{}.git",
        provider.instance.as_str(),
        policy.project_path
    );
    let git_bounds = GitFetchBounds::new(limits.git.request)
        .ok_or(ConfigError::invalid("GitLab Git timeout is invalid"))?;
    let objects = Arc::new(
        GitLabGitObjects::new(
            paths.scratch.clone(),
            policy.project_id,
            repository_url,
            raw.gitlab.git.username.clone(),
            clone_secret(&git_token),
            limits.git.request,
        )
        .ok_or(ConfigError::invalid("GitLab Git credential is invalid"))?,
    );
    let timeouts = GitLabTimeouts::new(
        limits.http.connect,
        operation_timeout(limits.http),
        PROVIDER_RESPONSE_BYTES,
    )
    .ok_or(ConfigError::invalid("GitLab API timeouts are invalid"))?;
    let client = GitLabClient::new(
        provider.clone(),
        &raw.gitlab.api_base,
        api_token,
        timeouts,
        objects,
    )
    .map_err(|defect| ConfigError::caused_by("GitLab API configuration is invalid", defect))?;
    let trust_set = TrustSetId::new(raw.gitlab.oidc.trust_set)
        .ok_or(ConfigError::invalid("GitLab OIDC trust set is invalid"))?;
    let source = Arc::new(
        GitLabOidc::new(
            provider.clone(),
            trust_set.clone(),
            raw.gitlab.oidc.issuer,
            raw.gitlab.oidc.audience,
            policy.clone(),
            keys(raw.gitlab.oidc.keys)?,
            limits.future_skew.as_secs(),
        )
        .map_err(|defect| ConfigError::caused_by("GitLab OIDC configuration is invalid", defect))?,
    );
    let ledger = FileLedgerConfig::new(limits.ledger.lease, limits.ledger.records, limits.replay)
        .ok_or(ConfigError::invalid(
        "GitLab delivery record limits are invalid",
    ))?;
    Ok(ServiceConfig {
        listen,
        evaluation,
        ledger,
        ingress: limits.ingress,
        route: DeliveryRoute {
            provider,
            trust_set,
            signed_time: SignedTimePolicy::Required(limits.signed_age),
        },
        source,
        client,
        project_id: policy.project_id,
        git_username: raw.gitlab.git.username,
        git_token,
        git_bounds,
        plan,
        scope,
        bootstrap: paths.bootstrap,
        scratch: paths.scratch,
        ledger_root: paths.ledger,
        bootstrap_timeout: limits.runner.bootstrap,
        statement_validity: limits.runner.statement_validity,
        artifacts,
    })
}

fn load_token(path: &std::path::Path) -> Result<SecretString, ConfigError> {
    let bytes = read_regular(path, TOKEN_BYTES)?;
    let token = String::from_utf8(bytes)
        .map_err(|defect| ConfigError::caused_by("GitLab token is invalid", defect))?;
    let valid = (16..=usize::try_from(TOKEN_BYTES).unwrap_or(usize::MAX)).contains(&token.len())
        && token.bytes().all(|byte| byte.is_ascii_graphic());
    valid
        .then(|| SecretString::from(token))
        .ok_or(ConfigError::invalid("GitLab token is invalid"))
}

fn policy_job_endpoint(
    loaded: amiss_controller_service::EndpointConfig,
) -> amiss_controller_service::EndpointConfig {
    amiss_controller_service::EndpointConfig {
        path: loaded.path,
        max_body_bytes: loaded.max_body_bytes.min(HINT_BODY_BYTES),
        max_headers: loaded.max_headers.min(POLICY_JOB_HEADERS),
        max_header_bytes: loaded.max_header_bytes.min(POLICY_JOB_HEADER_BYTES),
        max_concurrent_requests: loaded.max_concurrent_requests,
    }
}

fn clone_secret(secret: &SecretString) -> SecretString {
    use secrecy::ExposeSecret as _;
    SecretString::from(secret.expose_secret().to_owned())
}

fn operation_timeout(limits: amiss_controller_service::HttpLimits) -> Duration {
    limits.read.min(limits.write).min(limits.request)
}

fn validate_action(
    provider: &amiss_controller::ProviderIdentity,
    plan: &amiss_controller::CheckPlan,
) -> Result<(), ConfigError> {
    (plan.execution.action_repository().host() == provider.instance.as_str()
        && plan.execution.action_object_format() == ObjectFormat::Sha1)
        .then_some(())
        .ok_or(ConfigError::invalid(
            "action repository must use this SHA-1 GitLab instance",
        ))
}
