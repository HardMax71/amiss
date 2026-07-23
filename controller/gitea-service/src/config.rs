use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    CheckPlan, DeliveryRoute, GiteaWebhook, IngressPolicy, IntegrationId, PlanScope,
    ProviderIdentity, ReplayWindow, SignedTimePolicy, TrustSetId,
};
use amiss_controller_gitea::{DedicatedReviewer, GiteaClient, GiteaTimeouts};
pub use amiss_controller_service::ConfigError;
use amiss_controller_service::{
    CheckPlanFiles, HttpLimits, InboxLimits, ServiceLimits, ServicePaths, WebhookKeyFile,
    framed_route_id, load_limits, load_paths, load_plan, load_webhook_keyring, read_regular,
    read_strict_json,
};
use amiss_wire::model::{BranchRef, ObjectFormat, RepositoryIdentity};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;

const ROUTE_DOMAIN: &str = "amiss/controller-gitea-family-service-route-v1";
const TOKEN_BYTES: u64 = 4_096;

pub struct ServiceConfig {
    pub(crate) listen: SocketAddr,
    pub(crate) receiver: amiss_controller_service::ReceiverConfig,
    pub(crate) inbox: InboxLimits,
    pub(crate) ledger_lease: Duration,
    pub(crate) ledger_records: u64,
    pub(crate) ingress: IngressPolicy,
    pub(crate) replay: ReplayWindow,
    pub(crate) route: DeliveryRoute,
    pub(crate) route_id: String,
    pub(crate) provider: ProviderIdentity,
    pub(crate) reviewer: DedicatedReviewer,
    pub(crate) repository_id: u64,
    pub(crate) target: BranchRef,
    pub(crate) api_base: String,
    pub(crate) token: SecretString,
    pub(crate) webhook: GiteaWebhook,
    pub(crate) api_timeouts: GiteaTimeouts,
    pub(crate) git_timeout: Duration,
    pub(crate) plan: Arc<CheckPlan>,
    pub(crate) scope: PlanScope,
    pub(crate) bootstrap: PathBuf,
    pub(crate) scratch: PathBuf,
    pub(crate) inbox_root: PathBuf,
    pub(crate) ledger_root: PathBuf,
    pub(crate) bootstrap_timeout: Duration,
    pub(crate) statement_validity: Duration,
    pub(crate) retry_min: Duration,
    pub(crate) retry_max: Duration,
    pub(crate) idle_poll: Duration,
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
        let api_timeouts = GiteaTimeouts::new(limits.http.connect, operation_timeout(limits.http))
            .ok_or(ConfigError("Gitea-family API timeouts are invalid"))?;
        validate_client(
            &provider,
            &reviewer,
            &token,
            &self.provider.api_base,
            &plan,
            api_timeouts,
        )?;

        let trust_set = TrustSetId::new("gitea-family-webhook-keys".to_owned())
            .ok_or(ConfigError("trust set identity is invalid"))?;
        let route = DeliveryRoute {
            provider: provider.clone(),
            trust_set: trust_set.clone(),
            signed_time: SignedTimePolicy::ReplayOnly,
        };
        let reviewer_id = reviewer.id.to_string();
        let repository_id_field = repository_id.to_string();
        let plan_digest = plan.digest.to_string();
        let route_id = framed_route_id(
            ROUTE_DOMAIN,
            "gitea-family",
            &[
                provider.namespace.as_str(),
                provider.instance.as_str(),
                &reviewer_id,
                &reviewer.login,
                &repository_id_field,
                &repository.owner,
                &repository.name,
                target.as_str(),
                &plan_digest,
            ],
        )
        .ok_or(ConfigError("route identity is invalid"))?;
        let webhook =
            GiteaWebhook::new(load_webhook_keyring(trust_set, self.provider.webhook_keys)?);
        let scope = PlanScope {
            provider: provider.clone(),
            integration: reviewer_integration(reviewer.id)?,
            repository,
        };
        let paths = load_paths(&self.paths, &plan)?;

        Ok(ServiceConfig {
            listen,
            receiver: limits.receiver,
            inbox: limits.inbox,
            ledger_lease: limits.ledger.lease,
            ledger_records: limits.ledger.records,
            ingress: limits.ingress,
            replay: limits.replay,
            route,
            route_id,
            provider,
            reviewer,
            repository_id,
            target,
            api_base: self.provider.api_base,
            token,
            webhook,
            api_timeouts,
            git_timeout: limits.git.request,
            plan,
            scope,
            bootstrap: paths.bootstrap,
            scratch: paths.scratch,
            inbox_root: paths.inbox,
            ledger_root: paths.ledger,
            bootstrap_timeout: limits.runner.bootstrap,
            statement_validity: limits.runner.statement_validity,
            retry_min: limits.worker.retry_min,
            retry_max: limits.worker.retry_max,
            idle_poll: limits.worker.idle_poll,
        })
    }
}

fn socket_address(raw: &str) -> Result<SocketAddr, ConfigError> {
    raw.parse()
        .map_err(|_defect| ConfigError("listen must be one socket address"))
}

fn provider_identity(raw: &RawProvider) -> Result<ProviderIdentity, ConfigError> {
    ProviderIdentity::new(raw.namespace.clone(), raw.instance.clone())
        .ok_or(ConfigError("provider identity is invalid"))
}

fn dedicated_reviewer(raw: &RawReviewer) -> Result<DedicatedReviewer, ConfigError> {
    DedicatedReviewer::new(raw.id, raw.login.clone())
        .ok_or(ConfigError("dedicated reviewer identity is invalid"))
}

fn repository_identity(
    provider: &ProviderIdentity,
    raw: RawRepository,
) -> Result<RepositoryIdentity, ConfigError> {
    let canonical = raw.owner == raw.owner.to_ascii_lowercase()
        && raw.name == raw.name.to_ascii_lowercase()
        && !raw.owner.contains('/');
    if !canonical {
        return Err(ConfigError(
            "Gitea-family repository spelling is not canonical",
        ));
    }
    RepositoryIdentity::new(provider.instance.as_str().to_owned(), raw.owner, raw.name)
        .ok_or(ConfigError("Gitea-family repository identity is invalid"))
}

fn target_branch(raw: &str) -> Result<BranchRef, ConfigError> {
    (!raw.starts_with("refs/"))
        .then(|| BranchRef::new(format!("refs/heads/{raw}")))
        .flatten()
        .ok_or(ConfigError("Gitea-family target branch is invalid"))
}

fn reviewer_integration(id: u64) -> Result<IntegrationId, ConfigError> {
    IntegrationId::new(id.to_string())
        .ok_or(ConfigError("dedicated reviewer integration is invalid"))
}

fn load_token(path: &Path) -> Result<SecretString, ConfigError> {
    let bytes = read_regular(path, TOKEN_BYTES)?;
    let token =
        String::from_utf8(bytes).map_err(|_defect| ConfigError("provider token is invalid"))?;
    let valid = (16..=usize::try_from(TOKEN_BYTES).unwrap_or(usize::MAX)).contains(&token.len())
        && token.bytes().all(|byte| byte.is_ascii_graphic());
    valid
        .then(|| SecretString::from(token))
        .ok_or(ConfigError("provider token is invalid"))
}

fn validate_action(provider: &ProviderIdentity, plan: &CheckPlan) -> Result<(), ConfigError> {
    (plan.execution.action_repository.host == provider.instance.as_str()
        && !plan.execution.action_repository.owner.contains('/')
        && plan.execution.action_object_format == ObjectFormat::Sha1)
        .then_some(())
        .ok_or(ConfigError(
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
) -> Result<(), ConfigError> {
    GiteaClient::new(
        provider.clone(),
        reviewer.clone(),
        token.expose_secret().to_owned(),
        api_base,
        plan.execution.required_status_name.clone(),
        timeouts,
    )
    .map(|_client| ())
    .map_err(|_defect| ConfigError("Gitea-family API configuration is invalid"))
}

fn operation_timeout(limits: HttpLimits) -> Duration {
    limits.read.min(limits.write).min(limits.request)
}

fn positive(raw: u64) -> Result<u64, ConfigError> {
    (raw > 0).then_some(raw).ok_or(ConfigError(
        "Gitea-family numeric identity must be positive",
    ))
}
