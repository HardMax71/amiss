use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::objects::GiteaGitObjects;
use amiss_controller::{
    CheckPlan, DeliveryRoute, GiteaWebhook, IntegrationId, PlanScope, ProviderIdentity,
    SignedTimePolicy, TrustSetId,
};
use amiss_controller_gitea::{
    DedicatedReviewer, GiteaClient, GiteaObjectResolver, GiteaTimeouts, gitea_repository_url,
};
pub use amiss_controller_service::ConfigError;
use amiss_controller_service::{
    AcquiringWorkerSettings, ArtifactFiles, CheckPlanFiles, HttpLimits, QueuedLaneSetupInput,
    QueuedServiceSettings, ServiceLimits, ServicePaths, WebhookKeyFile, framed_route_id,
    load_artifact_service, load_limits, load_paths, load_plan, load_webhook_keyring, read_regular,
    read_strict_json,
};
use amiss_wire::model::{BranchRef, ObjectFormat, RepositoryIdentity};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

const ROUTE_DOMAIN: &str = "amiss/controller-gitea-family-service-route-v1";
const TOKEN_BYTES: u64 = 4_096;
const INVALID_GIT_CREDENTIAL: &str = "Gitea-family Git credential is invalid";
const INVALID_API_TIMEOUTS: &str = "Gitea-family API timeouts are invalid";

pub struct ServiceConfig {
    pub(crate) lane: QueuedLaneSetupInput,
    pub(crate) worker: AcquiringWorkerSettings,
    pub(crate) provider: ProviderIdentity,
    pub(crate) reviewer: DedicatedReviewer,
    pub(crate) repository_id: u64,
    pub(crate) objects: Arc<dyn GiteaObjectResolver>,
    pub(crate) target: BranchRef,
    pub(crate) api_base: String,
    pub(crate) token: SecretString,
    pub(crate) webhook: GiteaWebhook,
    pub(crate) api_timeouts: GiteaTimeouts,
    pub(crate) git_timeout: Duration,
    pub(crate) review_name: String,
}

impl ServiceConfig {
    /// Loads one closed configuration and every external trust input it names.
    ///
    /// # Errors
    ///
    /// The config, a trust file, an identity, a bound plan, or a limit is invalid.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let raw: RawConfig = read_strict_json(path)?;
        raw.load()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    listen: String,
    webhook_path: String,
    provider: RawProvider,
    repository: RawRepository,
    plan: CheckPlanFiles,
    paths: ServicePaths,
    artifacts: ArtifactFiles,
    #[serde(default)]
    limits: ServiceLimits,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProvider {
    namespace: String,
    instance: String,
    api_base: String,
    reviewer: RawReviewer,
    webhook_keys: Vec<WebhookKeyFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReviewer {
    id: u64,
    login: String,
    token_file: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRepository {
    id: u64,
    owner: String,
    name: String,
    target_branch: String,
}

impl RawConfig {
    fn load(self) -> Result<ServiceConfig, ConfigError> {
        let listen = socket_address(&self.listen)?;
        let provider = provider_identity(&self.provider)?;
        let reviewer = dedicated_reviewer(&self.provider.reviewer)?;
        let repository_id = positive(self.repository.id)?;
        let target = target_branch(&self.repository.target_branch)?;
        let repository = repository_identity(&provider, self.repository)?;
        let plan = Arc::new(load_plan(&self.plan)?);
        validate_action(&provider, &plan)?;
        let limits = load_limits(&self.limits, self.webhook_path)?;
        let token = load_token(&self.provider.reviewer.token_file)?;
        let paths = load_paths(&self.paths, &plan)?;
        let artifacts = load_artifact_service(
            &self.artifacts,
            paths.artifacts.clone(),
            limits.artifacts,
            &limits.receiver,
        )?;
        let objects: Arc<dyn GiteaObjectResolver> = Arc::new(
            GiteaGitObjects::new(
                paths.scratch.clone(),
                repository_id,
                gitea_repository_url(&repository),
                reviewer.login.clone(),
                token.clone(),
                limits.git.request,
            )
            .ok_or(ConfigError::invalid(INVALID_GIT_CREDENTIAL))?,
        );
        let api_timeouts = GiteaTimeouts::new(limits.http.connect, operation_timeout(limits.http))
            .ok_or(ConfigError::invalid(INVALID_API_TIMEOUTS))?;
        validate_client(
            &provider,
            &reviewer,
            &token,
            &self.provider.api_base,
            &plan,
            api_timeouts,
            Arc::clone(&objects),
        )?;

        let (route, webhook) = webhook_binding(&provider, self.provider.webhook_keys)?;
        let route_id = gitea_route_id(
            &provider,
            &reviewer,
            repository_id,
            &repository,
            &target,
            &plan,
        )?;
        let scope = PlanScope {
            provider: provider.clone(),
            integration: reviewer_integration(reviewer.id)?,
            repository,
        };
        let review_name = plan.execution.required_status_name().to_owned();
        let worker = AcquiringWorkerSettings {
            bootstrap: paths.bootstrap,
            scratch: paths.scratch,
            bootstrap_timeout: limits.runner.bootstrap,
            statement_validity: limits.runner.statement_validity,
            ingress: limits.ingress,
            route,
            route_id,
            retry_min: limits.worker.retry_min,
            retry_max: limits.worker.retry_max,
            idle_poll: limits.worker.idle_poll,
        };
        let lane = QueuedLaneSetupInput {
            service: QueuedServiceSettings {
                listen,
                receiver: limits.receiver,
                inbox_root: paths.inbox,
                inbox_limits: limits.inbox,
            },
            plan,
            scope,
            ledger_root: paths.ledger,
            ledger_lease: limits.ledger.lease,
            ledger_records: limits.ledger.records,
            replay: limits.replay,
            artifacts,
        };

        Ok(ServiceConfig {
            lane,
            worker,
            provider,
            reviewer,
            repository_id,
            objects,
            target,
            api_base: self.provider.api_base,
            token,
            webhook,
            api_timeouts,
            git_timeout: limits.git.request,
            review_name,
        })
    }
}

fn webhook_binding(
    provider: &ProviderIdentity,
    keys: Vec<WebhookKeyFile>,
) -> Result<(DeliveryRoute, GiteaWebhook), ConfigError> {
    let trust_set = TrustSetId::new("gitea-family-webhook-keys".to_owned())
        .ok_or(ConfigError::invalid("trust set identity is invalid"))?;
    let route = DeliveryRoute {
        provider: provider.clone(),
        trust_set: trust_set.clone(),
        signed_time: SignedTimePolicy::ReplayOnly,
    };
    let webhook = GiteaWebhook::new(load_webhook_keyring(trust_set, keys)?);
    Ok((route, webhook))
}

fn gitea_route_id(
    provider: &ProviderIdentity,
    reviewer: &DedicatedReviewer,
    repository_id: u64,
    repository: &RepositoryIdentity,
    target: &BranchRef,
    plan: &CheckPlan,
) -> Result<String, ConfigError> {
    let reviewer_id = reviewer.id.to_string();
    let repository_id = repository_id.to_string();
    let plan_digest = plan.digest.to_string();
    framed_route_id(
        ROUTE_DOMAIN,
        "gitea-family",
        &[
            provider.namespace.as_str(),
            provider.instance.as_str(),
            &reviewer_id,
            &reviewer.login,
            &repository_id,
            repository.owner(),
            repository.name(),
            target.as_str(),
            &plan_digest,
        ],
    )
    .ok_or(ConfigError::invalid("route identity is invalid"))
}

fn socket_address(raw: &str) -> Result<SocketAddr, ConfigError> {
    raw.parse()
        .map_err(|defect| ConfigError::caused_by("listen must be one socket address", defect))
}

fn provider_identity(raw: &RawProvider) -> Result<ProviderIdentity, ConfigError> {
    ProviderIdentity::new(raw.namespace.clone(), raw.instance.clone())
        .ok_or(ConfigError::invalid("provider identity is invalid"))
}

fn dedicated_reviewer(raw: &RawReviewer) -> Result<DedicatedReviewer, ConfigError> {
    DedicatedReviewer::new(raw.id, raw.login.clone()).ok_or(ConfigError::invalid(
        "dedicated reviewer identity is invalid",
    ))
}

fn repository_identity(
    provider: &ProviderIdentity,
    raw: RawRepository,
) -> Result<RepositoryIdentity, ConfigError> {
    let canonical = raw.owner == raw.owner.to_ascii_lowercase()
        && raw.name == raw.name.to_ascii_lowercase()
        && !raw.owner.contains('/');
    if !canonical {
        return Err(ConfigError::invalid(
            "Gitea-family repository spelling is not canonical",
        ));
    }
    RepositoryIdentity::new(provider.instance.as_str().to_owned(), raw.owner, raw.name).ok_or(
        ConfigError::invalid("Gitea-family repository identity is invalid"),
    )
}

fn target_branch(raw: &str) -> Result<BranchRef, ConfigError> {
    (!raw.starts_with("refs/"))
        .then(|| BranchRef::new(format!("refs/heads/{raw}")))
        .flatten()
        .ok_or(ConfigError::invalid(
            "Gitea-family target branch is invalid",
        ))
}

fn reviewer_integration(id: u64) -> Result<IntegrationId, ConfigError> {
    IntegrationId::new(id.to_string()).ok_or(ConfigError::invalid(
        "dedicated reviewer integration is invalid",
    ))
}

fn load_token(path: &Path) -> Result<SecretString, ConfigError> {
    let bytes = read_regular(path, TOKEN_BYTES)?;
    let token = String::from_utf8(bytes)
        .map_err(|defect| ConfigError::caused_by("provider token is invalid", defect))?;
    let valid = (16..=usize::try_from(TOKEN_BYTES).unwrap_or(usize::MAX)).contains(&token.len())
        && token.bytes().all(|byte| byte.is_ascii_graphic());
    valid
        .then(|| SecretString::from(token))
        .ok_or(ConfigError::invalid("provider token is invalid"))
}

fn validate_action(provider: &ProviderIdentity, plan: &CheckPlan) -> Result<(), ConfigError> {
    (plan.execution.action_repository().host() == provider.instance.as_str()
        && !plan.execution.action_repository().owner().contains('/')
        && plan.execution.action_object_format() == ObjectFormat::Sha1)
        .then_some(())
        .ok_or(ConfigError::invalid(
            "action repository must use this SHA-1 provider instance",
        ))
}

fn validate_client(
    provider: &ProviderIdentity,
    reviewer: &DedicatedReviewer,
    token: &SecretString,
    api_base: &str,
    plan: &CheckPlan,
    timeouts: GiteaTimeouts,
    objects: Arc<dyn GiteaObjectResolver>,
) -> Result<(), ConfigError> {
    GiteaClient::new(
        provider.clone(),
        reviewer.clone(),
        token.expose_secret().to_owned(),
        api_base,
        plan.execution.required_status_name().to_owned(),
        timeouts,
        objects,
    )
    .map(|_client| ())
    .map_err(|defect| ConfigError::caused_by("Gitea-family API configuration is invalid", defect))
}

fn operation_timeout(limits: HttpLimits) -> Duration {
    limits.read.min(limits.write).min(limits.request)
}

fn positive(raw: u64) -> Result<u64, ConfigError> {
    (raw > 0).then_some(raw).ok_or(ConfigError::invalid(
        "Gitea-family numeric identity must be positive",
    ))
}
