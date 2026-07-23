use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{
    CheckPlan, DeliveryRoute, GitHubWebhook, IngressPolicy, IntegrationId, PlanScope,
    ProviderIdentity, ReplayWindow, SignedTimePolicy, TrustSetId,
};
use amiss_controller_github::GitHubTimeouts;
pub use amiss_controller_service::ConfigError;
use amiss_controller_service::{
    CheckPlanFiles, InboxLimits, ServiceLimits, ServicePaths, WebhookKeyFile, framed_route_id,
    load_limits, load_paths, load_plan, load_webhook_keyring, read_regular, read_strict_json,
};
use amiss_wire::model::{BranchRef, RepositoryIdentity};
use serde::Deserialize;

const PRIVATE_KEY_BYTES: u64 = 65_536;
const ROUTE_DOMAIN: &str = "amiss/controller-github-service-route-v1";

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
    pub(crate) app_id: u64,
    pub(crate) installation_id: u64,
    pub(crate) repository_id: u64,
    pub(crate) target: BranchRef,
    pub(crate) api_base: String,
    pub(crate) private_key: Vec<u8>,
    pub(crate) webhook: GitHubWebhook,
    pub(crate) api_timeouts: GitHubTimeouts,
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
    github: RawGitHub,
    repository: RawRepository,
    plan: CheckPlanFiles,
    paths: ServicePaths,
    #[serde(default)]
    limits: ServiceLimits,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGitHub {
    instance: String,
    api_base: String,
    app_id: u64,
    installation_id: u64,
    private_key_file: PathBuf,
    webhook_keys: Vec<WebhookKeyFile>,
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
        let listen = self
            .listen
            .parse()
            .map_err(|_defect| ConfigError("listen must be one socket address"))?;
        let scope = checked_scope(&self.github, self.repository)?;
        let plan = Arc::new(load_plan(&self.plan)?);
        let limits = load_limits(&self.limits, self.webhook_path)?;
        let trust_set = TrustSetId::new("github-webhook-keys".to_owned())
            .ok_or(ConfigError("trust set identity is invalid"))?;
        let route = DeliveryRoute {
            provider: scope.provider.clone(),
            trust_set: trust_set.clone(),
            signed_time: SignedTimePolicy::ReplayOnly,
        };
        let app_id = self.github.app_id.to_string();
        let installation_id = self.github.installation_id.to_string();
        let repository_id = scope.repository_id.to_string();
        let plan_digest = plan.digest.to_string();
        let route_id = framed_route_id(
            ROUTE_DOMAIN,
            "github",
            &[
                scope.provider.namespace.as_str(),
                scope.provider.instance.as_str(),
                &app_id,
                &installation_id,
                &repository_id,
                &scope.repository.owner,
                &scope.repository.name,
                scope.target.as_str(),
                &plan_digest,
            ],
        )
        .ok_or(ConfigError("route identity is invalid"))?;
        let webhook =
            GitHubWebhook::new(load_webhook_keyring(trust_set, self.github.webhook_keys)?);
        let api_timeouts = GitHubTimeouts::new(
            limits.http.connect,
            limits.http.read,
            limits.http.write,
            limits.http.request,
        )
        .ok_or(ConfigError("GitHub API timeouts are invalid"))?;
        let plan_scope = PlanScope {
            provider: scope.provider.clone(),
            integration: scope.integration,
            repository: scope.repository,
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
            provider: scope.provider,
            app_id: positive(self.github.app_id)?,
            installation_id: positive(self.github.installation_id)?,
            repository_id: positive(scope.repository_id)?,
            target: scope.target,
            api_base: self.github.api_base,
            private_key: read_regular(&self.github.private_key_file, PRIVATE_KEY_BYTES)?,
            webhook,
            api_timeouts,
            git_timeout: limits.git.request,
            plan,
            scope: plan_scope,
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

struct CheckedScope {
    provider: ProviderIdentity,
    integration: IntegrationId,
    repository: RepositoryIdentity,
    repository_id: u64,
    target: BranchRef,
}

fn checked_scope(
    github: &RawGitHub,
    repository: RawRepository,
) -> Result<CheckedScope, ConfigError> {
    let provider = github_provider(&github.instance)?;
    let repository_id = repository.id;
    let target = github_branch(&repository.target_branch)?;
    let repository = github_repository(&provider, repository)?;
    let integration = positive_id(github.installation_id)?;
    Ok(CheckedScope {
        provider,
        integration,
        repository,
        repository_id,
        target,
    })
}

fn github_provider(instance: &str) -> Result<ProviderIdentity, ConfigError> {
    let canonical = instance == instance.to_ascii_lowercase()
        && !instance.contains('/')
        && !instance.is_empty();
    if !canonical {
        return Err(ConfigError("GitHub instance is not canonical"));
    }
    ProviderIdentity::new("github".to_owned(), instance.to_owned())
        .ok_or(ConfigError("GitHub instance is invalid"))
}

fn github_repository(
    provider: &ProviderIdentity,
    repository: RawRepository,
) -> Result<RepositoryIdentity, ConfigError> {
    positive(repository.id)?;
    let canonical = repository.owner == repository.owner.to_ascii_lowercase()
        && repository.name == repository.name.to_ascii_lowercase()
        && !repository.owner.contains('/');
    if !canonical {
        return Err(ConfigError("GitHub repository spelling is not canonical"));
    }
    RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        repository.owner,
        repository.name,
    )
    .ok_or(ConfigError("GitHub repository identity is invalid"))
}

fn github_branch(branch: &str) -> Result<BranchRef, ConfigError> {
    (!branch.starts_with("refs/"))
        .then(|| BranchRef::new(format!("refs/heads/{branch}")))
        .flatten()
        .ok_or(ConfigError("GitHub target branch is invalid"))
}

fn positive_id(raw: u64) -> Result<IntegrationId, ConfigError> {
    positive(raw)?;
    IntegrationId::new(raw.to_string()).ok_or(ConfigError("installation identity is invalid"))
}

fn positive(raw: u64) -> Result<u64, ConfigError> {
    (raw > 0)
        .then_some(raw)
        .ok_or(ConfigError("GitHub numeric identity must be positive"))
}
