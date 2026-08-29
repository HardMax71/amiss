use std::path::{Path, PathBuf};
use std::sync::Arc;

use amiss_controller::{
    CheckPlan, DeliveryRoute, GitHubWebhook, IntegrationId, PlanScope, ProviderIdentity,
    SignedTimePolicy, TrustSetId,
};
use amiss_controller_github::{GitHubApp, GitHubTimeouts};
pub use amiss_controller_service::ConfigError;
use amiss_controller_service::{
    AcquiringWorkerSettings, ArtifactFiles, CheckPlanFiles, QueuedLaneSetupInput,
    QueuedServiceSettings, ServiceLimits, ServicePaths, WebhookKeyFile, framed_route_id,
    load_artifact_service, load_limits, load_paths, load_plan, load_webhook_keyring, read_regular,
    read_strict_json,
};
use amiss_wire::model::{BranchRef, ObjectFormat, RepositoryIdentity};
use serde::Deserialize;

const PRIVATE_KEY_BYTES: u64 = 65_536;
const ROUTE_DOMAIN: &str = "amiss/controller-github-service-route-v1";

pub struct ServiceConfig {
    pub(crate) lane: QueuedLaneSetupInput,
    pub(crate) worker: AcquiringWorkerSettings,
    pub(crate) provider: ProviderIdentity,
    pub(crate) app: GitHubApp,
    pub(crate) repository_id: u64,
    pub(crate) target: BranchRef,
    pub(crate) webhook: GitHubWebhook,
    pub(crate) git_timeout: std::time::Duration,
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
    artifacts: ArtifactFiles,
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
        let listen = self.listen.parse().map_err(|defect| {
            ConfigError::caused_by("listen must be one socket address", defect)
        })?;
        let scope = checked_scope(&self.github, self.repository)?;
        let plan = Arc::new(load_plan(
            &self.plan,
            Some((&scope.provider, &scope.repository)),
        )?);
        validate_github_plan(&scope.provider, &plan)?;
        let limits = load_limits(&self.limits, self.webhook_path)?;
        let trust_set = TrustSetId::new("github-webhook-keys".to_owned())
            .ok_or(ConfigError::invalid("trust set identity is invalid"))?;
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
                scope.repository.owner(),
                scope.repository.name(),
                scope.target.as_str(),
                &plan_digest,
            ],
        )
        .ok_or(ConfigError::invalid("route identity is invalid"))?;
        let webhook =
            GitHubWebhook::new(load_webhook_keyring(trust_set, self.github.webhook_keys)?);
        let api_timeouts = GitHubTimeouts::new(limits.http.connect, limits.http.request)
            .ok_or(ConfigError::invalid("GitHub API timeouts are invalid"))?;
        let app = GitHubApp::new(
            scope.provider.clone(),
            positive(self.github.app_id)?,
            positive(self.github.installation_id)?,
            read_regular(&self.github.private_key_file, PRIVATE_KEY_BYTES)?,
            &self.github.api_base,
            plan.execution.required_status_name().to_owned(),
            api_timeouts,
        )
        .map_err(|defect| ConfigError::caused_by("GitHub App configuration is invalid", defect))?;
        let plan_scope = PlanScope {
            provider: scope.provider.clone(),
            integration: scope.integration,
            repository: scope.repository,
        };
        let paths = load_paths(&self.paths, &plan)?;
        let artifacts = load_artifact_service(
            &self.artifacts,
            paths.artifacts.clone(),
            limits.artifacts,
            &limits.receiver,
        )?;
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
            scope: plan_scope,
            ledger_root: paths.ledger,
            ledger_lease: limits.ledger.lease,
            ledger_records: limits.ledger.records,
            replay: limits.replay,
            artifacts,
        };
        Ok(ServiceConfig {
            lane,
            worker,
            provider: scope.provider,
            app,
            repository_id: positive(scope.repository_id)?,
            target: scope.target,
            webhook,
            git_timeout: limits.git.request,
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
        return Err(ConfigError::invalid("GitHub instance is not canonical"));
    }
    ProviderIdentity::new("github".to_owned(), instance.to_owned())
        .ok_or(ConfigError::invalid("GitHub instance is invalid"))
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
        return Err(ConfigError::invalid(
            "GitHub repository spelling is not canonical",
        ));
    }
    RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        repository.owner,
        repository.name,
    )
    .ok_or(ConfigError::invalid(
        "GitHub repository identity is invalid",
    ))
}

fn github_branch(branch: &str) -> Result<BranchRef, ConfigError> {
    (!branch.starts_with("refs/"))
        .then(|| BranchRef::new(format!("refs/heads/{branch}")))
        .flatten()
        .ok_or(ConfigError::invalid("GitHub target branch is invalid"))
}

fn validate_github_plan(provider: &ProviderIdentity, plan: &CheckPlan) -> Result<(), ConfigError> {
    (plan.execution.action_repository().host() == provider.instance.as_str()
        && !plan.execution.action_repository().owner().contains('/')
        && plan.execution.action_object_format() == ObjectFormat::Sha1)
        .then_some(())
        .ok_or(ConfigError::invalid(
            "action repository must use this SHA-1 GitHub instance",
        ))?;
    plan.policy
        .workflow_artifacts
        .first()
        .is_none_or(|first| {
            plan.policy.workflow_artifacts.iter().all(|artifact| {
                artifact.workflow_identity == first.workflow_identity
                    && artifact.event == first.event
            })
        })
        .then_some(())
        .ok_or(ConfigError::invalid(
            "workflow artifacts must use one completion trigger",
        ))
}

fn positive_id(raw: u64) -> Result<IntegrationId, ConfigError> {
    positive(raw)?;
    IntegrationId::new(raw.to_string())
        .ok_or(ConfigError::invalid("installation identity is invalid"))
}

fn positive(raw: u64) -> Result<u64, ConfigError> {
    (raw > 0).then_some(raw).ok_or(ConfigError::invalid(
        "GitHub numeric identity must be positive",
    ))
}
