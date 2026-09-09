use crate::claims::{Claims, RequestHint};
use crate::identity::{canonical_project_path, exact_sha1, repository_identity};
use amiss_controller::{
    AuthenticatedDelivery, ChangeId, ChangeLocator, DeliveryId, DeliveryIdentity, ProviderError,
    ProviderIdentity, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity,
};
use amiss_wire::model::ObjectFormat;

use super::PolicyBinding;

const POLICY_JOB_SOURCE: &str = "pipeline_execution_policy";
const MERGE_REQUEST_PIPELINE: &str = "merge_request_event";

pub(crate) struct AuthenticatedFacts {
    pub delivery: AuthenticatedDelivery,
    pub replay: DeliveryId,
    pub issued_at_unix_millis: i64,
}

pub(crate) fn authenticated_facts(
    provider: &ProviderIdentity,
    policy: &PolicyBinding,
    claims: &Claims,
    body: &[u8],
    now_seconds: u64,
    skew_seconds: u64,
) -> Result<AuthenticatedFacts, ProviderError> {
    let hint: RequestHint =
        serde_json::from_slice(body).map_err(|_defect| ProviderError::Authentication)?;
    let gate = exact_sha1(&claims.sha).ok_or(ProviderError::Authentication)?;
    let project_path =
        canonical_project_path(&claims.job_project_path).ok_or(ProviderError::Authentication)?;
    let runner_authorized = claims.runner_environment == "gitlab-hosted"
        && policy.runners.gitlab_hosted
        || claims.runner_environment == "self-hosted"
            && policy.runners.self_hosted_ids.contains(&claims.runner_id);
    if claims.aud.is_empty()
        || claims.sub.is_empty()
        || claims.jti.is_empty()
        || claims.jti.len() > 1_024
        || !claims.jti.bytes().all(|byte| byte.is_ascii_graphic())
        || claims.job_project_id != policy.project_id
        || project_path != policy.project_path
        || claims.pipeline_source != MERGE_REQUEST_PIPELINE
        || claims.job_source != POLICY_JOB_SOURCE
        || claims.job_config.url != policy.config_url
        || claims.job_config.sha != policy.config_commit.as_str()
        || claims.pipeline_id == 0
        || claims.job_id == 0
        || claims.runner_id == 0
        || !runner_authorized
        || claims.iat > claims.exp
        || claims.nbf > claims.exp
        || claims.exp.saturating_add(skew_seconds) < now_seconds
        || claims.nbf > now_seconds.saturating_add(skew_seconds)
        || hint.merge_request_iid == 0
    {
        return Err(ProviderError::Authentication);
    }
    let issued_at_unix_millis = i64::try_from(claims.iat)
        .ok()
        .and_then(|value| value.checked_mul(1_000))
        .ok_or(ProviderError::Authentication)?;
    let repository = repository_identity(provider.instance.as_str(), &project_path)
        .ok_or(ProviderError::Authentication)?;
    let change = ChangeLocator {
        provider: provider.clone(),
        repository,
        change: change_id(policy.project_id, hint.merge_request_iid)
            .ok_or(ProviderError::Authentication)?,
    };
    let provider_run = ProviderRunIdentity::new(
        ProviderRunId::try_from(format!(
            "pipeline/{}/job/{}",
            claims.pipeline_id, claims.job_id
        ))
        .map_err(|_error| ProviderError::Authentication)?,
        ProviderRunAttempt::try_from(1).map_err(|_error| ProviderError::Authentication)?,
        ObjectFormat::Sha1,
        gate,
    )
    .ok_or(ProviderError::Authentication)?;
    let digest = amiss_wire::digest::hb("amiss/gitlab-oidc-jti-v1", claims.jti.as_bytes());
    let replay = DeliveryId::try_from(format!("oidc/runner/{}/jti/{digest}", claims.runner_id))
        .map_err(|_error| ProviderError::Authentication)?;
    Ok(AuthenticatedFacts {
        delivery: AuthenticatedDelivery {
            identity: DeliveryIdentity {
                provider: provider.clone(),
                integration: policy.integration.clone(),
                delivery: DeliveryId::try_from("pending".to_owned())
                    .map_err(|_error| ProviderError::Authentication)?,
            },
            change,
            provider_run,
        },
        replay,
        issued_at_unix_millis,
    })
}

fn change_id(project_id: u64, merge_request_iid: u64) -> Option<ChangeId> {
    ChangeId::try_from(format!(
        "project/{project_id}/merge-request/{merge_request_iid}"
    ))
    .ok()
}
