use std::collections::BTreeSet;

use amiss_controller::{IntegrationId, PlanScope, ProviderIdentity, TrustAnchorId};
use amiss_controller_gitlab::{OidcPublicKey, PolicyBinding, RunnerTrust};
use amiss_controller_service::{ConfigError, read_regular};
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

use super::raw::{RawOidcKey, RawPolicy};

const PUBLIC_KEY_BYTES: u64 = 65_536;

pub(super) fn provider(instance: String) -> Result<ProviderIdentity, ConfigError> {
    ProviderIdentity::new("gitlab".to_owned(), instance)
        .ok_or(ConfigError("GitLab instance is invalid"))
}

pub(super) fn policy(raw: RawPolicy) -> Result<PolicyBinding, ConfigError> {
    let integration = IntegrationId::new(raw.integration)
        .ok_or(ConfigError("GitLab policy integration is invalid"))?;
    let config_commit = Oid::new(ObjectFormat::Sha1, raw.config_commit)
        .ok_or(ConfigError("GitLab policy commit is invalid"))?;
    let runner_count = raw.self_hosted_runner_ids.len();
    let self_hosted_ids = raw
        .self_hosted_runner_ids
        .into_iter()
        .collect::<BTreeSet<_>>();
    let runners_valid =
        self_hosted_ids.len() == runner_count && self_hosted_ids.iter().all(|runner| *runner > 0);
    runners_valid
        .then_some(PolicyBinding {
            integration,
            project_id: raw.project_id,
            project_path: raw.project_path,
            target_branch: raw.target_branch,
            job_name: raw.job_name,
            config_url: raw.config_url,
            config_commit,
            runners: RunnerTrust {
                gitlab_hosted: raw.gitlab_hosted_runners,
                self_hosted_ids,
            },
        })
        .ok_or(ConfigError("GitLab runner trust is invalid"))
}

pub(super) fn keys(raw: Vec<RawOidcKey>) -> Result<Vec<OidcPublicKey>, ConfigError> {
    raw.into_iter()
        .map(|key| {
            let anchor = TrustAnchorId::new(key.anchor)
                .ok_or(ConfigError("GitLab OIDC trust anchor is invalid"))?;
            let pem = read_regular(&key.public_key_file, PUBLIC_KEY_BYTES)?;
            OidcPublicKey::from_rsa_pem(key.kid, anchor, &pem)
                .map_err(|_defect| ConfigError("GitLab OIDC public key is invalid"))
        })
        .collect()
}

pub(super) fn scope(
    provider: &ProviderIdentity,
    policy: &PolicyBinding,
) -> Result<PlanScope, ConfigError> {
    let (owner, name) = policy
        .project_path
        .rsplit_once('/')
        .ok_or(ConfigError("GitLab project path is invalid"))?;
    let repository = RepositoryIdentity::new(
        provider.instance.as_str().to_owned(),
        owner.to_owned(),
        name.to_owned(),
    )
    .ok_or(ConfigError("GitLab project identity is invalid"))?;
    Ok(PlanScope {
        provider: provider.clone(),
        integration: policy.integration.clone(),
        repository,
    })
}
