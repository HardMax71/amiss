mod identity;
mod load;
mod raw;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use amiss_controller::{CheckPlan, DeliveryRoute, FileLedgerConfig, IngressPolicy, PlanScope};
use amiss_controller_git::GitFetchBounds;
use amiss_controller_gitlab::{GitLabClient, GitLabOidc};
pub use amiss_controller_service::ConfigError;
use amiss_controller_service::EvaluationConfig;
use secrecy::SecretString;

pub struct ServiceConfig {
    pub(crate) listen: SocketAddr,
    pub(crate) evaluation: EvaluationConfig,
    pub(crate) ledger: FileLedgerConfig,
    pub(crate) ingress: IngressPolicy,
    pub(crate) route: DeliveryRoute,
    pub(crate) source: Arc<GitLabOidc>,
    pub(crate) client: GitLabClient,
    pub(crate) project_id: u64,
    pub(crate) git_username: String,
    pub(crate) git_token: SecretString,
    pub(crate) git_bounds: GitFetchBounds,
    pub(crate) plan: Arc<CheckPlan>,
    pub(crate) scope: PlanScope,
    pub(crate) bootstrap: PathBuf,
    pub(crate) scratch: PathBuf,
    pub(crate) ledger_root: PathBuf,
    pub(crate) bootstrap_timeout: Duration,
    pub(crate) statement_validity: Duration,
}

impl ServiceConfig {
    /// Loads one closed configuration and every external trust input it names.
    ///
    /// # Errors
    ///
    /// The config, credential, key, identity, plan, path, or bound limit is invalid.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let raw: raw::RawConfig = amiss_controller_service::read_strict_json(path)?;
        load::load(raw)
    }
}
