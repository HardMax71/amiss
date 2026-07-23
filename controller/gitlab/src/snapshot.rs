use amiss_controller::{
    AuthenticatedDelivery, ChangeSnapshot, ChangeState, CheckConclusion, OidPair, ProviderError,
    RunFailure, RunIdentity, RunRefs,
};
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid};

use crate::identity::{branch_ref, canonical_project_path, exact_sha1, repository_url, train_ref};
use crate::{GitLabRefresh, GitLabRefreshQuery, GitLabTrainCar, PolicyBinding};

const MERGE_REQUEST_PIPELINE: &str = "merge_request_event";
const POLICY_JOB_SOURCE: &str = "pipeline_execution_policy";
const TRAIN_ENFORCEMENT: &str = "enforce_for_all_users";

pub(crate) fn snapshot(
    delivery: &AuthenticatedDelivery,
    policy: &PolicyBinding,
    query: &GitLabRefreshQuery,
    refresh: &GitLabRefresh,
) -> Result<ChangeSnapshot, ProviderError> {
    let gate = exact_oid(&refresh.gate.id)?;
    let gate_tree = exact_oid(&refresh.gate.tree)?;
    let base = exact_oid(&refresh.base.id)?;
    let base_tree = exact_oid(&refresh.base.tree)?;
    let target = exact_oid(&refresh.target.commit)?;
    let source = exact_oid(&refresh.merge_request.sha)?;
    let [first_parent, second_parent] = refresh.gate.parents.as_slice() else {
        return Err(ProviderError::InvalidResponse);
    };
    let first_parent = exact_oid(first_parent)?;
    let second_parent = exact_oid(second_parent)?;
    let parents_valid = [&refresh.base, &refresh.gate]
        .into_iter()
        .flat_map(|commit| &commit.parents)
        .all(|parent| exact_oid(parent).is_ok());
    let records_valid = validate_project(delivery, policy, query, refresh)?
        && refresh.job.id == query.job_id
        && refresh.job.name == policy.job_name
        && refresh.job.source == POLICY_JOB_SOURCE
        && refresh.job.pipeline_id == query.pipeline_id
        && refresh.job.commit == query.gate_commit.as_str()
        && refresh.job.runner_id == query.runner_id
        && refresh.pipeline.id == query.pipeline_id
        && refresh.pipeline.project_id == query.project_id
        && refresh.pipeline.sha == query.gate_commit.as_str()
        && refresh.pipeline.reference == train_ref(query.merge_request_iid)
        && refresh.pipeline.source == MERGE_REQUEST_PIPELINE
        && refresh.merge_request.iid == query.merge_request_iid
        && refresh.merge_request.project_id == query.project_id
        && refresh.merge_request.target_project_id == query.project_id
        && refresh.merge_request.target_branch == policy.target_branch
        && refresh.target.name == policy.target_branch
        && gate == query.gate_commit
        && first_parent == base
        && second_parent == source
        && target != gate
        && parents_valid;
    if !records_valid {
        return Err(ProviderError::InvalidResponse);
    }

    let train_live = refresh
        .train
        .as_ref()
        .map(|train| train_matches(query, policy, train))
        .transpose()?
        .unwrap_or(false);
    let open = match refresh.merge_request.state.as_str() {
        "opened" => true,
        "closed" | "locked" | "merged" => false,
        _ => return Err(ProviderError::InvalidResponse),
    };
    let refs = RunRefs {
        forge: ForgeDialect::Gitlab,
        candidate: branch_ref(&refresh.merge_request.source_branch)
            .ok_or(ProviderError::InvalidResponse)?,
        target: branch_ref(&refresh.merge_request.target_branch)
            .ok_or(ProviderError::InvalidResponse)?,
        default_branch: branch_ref(&refresh.project.default_branch)
            .ok_or(ProviderError::InvalidResponse)?,
    };
    let run = RunIdentity::new(
        delivery.change.clone(),
        refs,
        ObjectFormat::Sha1,
        OidPair {
            base,
            candidate: gate.clone(),
        },
        OidPair {
            base: base_tree,
            candidate: gate_tree,
        },
    )
    .ok_or(ProviderError::InvalidResponse)?;
    let live = refresh.job.status == "running"
        && refresh.pipeline.status == "running"
        && train_live
        && open
        && !refresh.merge_request.draft;
    let state = if !policy_authorized(policy, refresh) {
        ChangeState::AuthorizationRevoked
    } else if !open {
        ChangeState::Closed
    } else if live {
        ChangeState::Active
    } else {
        ChangeState::Superseded
    };
    Ok(ChangeSnapshot {
        state,
        run,
        gate_commit: gate,
    })
}

pub(crate) fn conclusion_matches(state: ChangeState, conclusion: CheckConclusion) -> bool {
    match conclusion {
        CheckConclusion::Pass | CheckConclusion::Block => state == ChangeState::Active,
        CheckConclusion::Superseded => state == ChangeState::Superseded,
        CheckConclusion::Unavailable(failure) => match failure {
            RunFailure::AuthorizationRevoked => state == ChangeState::AuthorizationRevoked,
            RunFailure::Closed => state == ChangeState::Closed,
            RunFailure::MissingOutput
            | RunFailure::Timeout
            | RunFailure::TamperedRuntime
            | RunFailure::Unavailable
            | RunFailure::OversizedOutput
            | RunFailure::WrongIdentity
            | RunFailure::WrongTree => state == ChangeState::Active,
        },
    }
}

fn validate_project(
    delivery: &AuthenticatedDelivery,
    policy: &PolicyBinding,
    query: &GitLabRefreshQuery,
    refresh: &GitLabRefresh,
) -> Result<bool, ProviderError> {
    let project_path = canonical_project_path(&refresh.project.path_with_namespace)
        .ok_or(ProviderError::InvalidResponse)?;
    let project_url = repository_url(delivery.change.provider.instance.as_str(), &project_path)
        .ok_or(ProviderError::InvalidResponse)?;
    Ok(refresh.project.id == query.project_id
        && project_path == policy.project_path
        && refresh.project.http_url_to_repo == project_url
        && refresh.project.repository_object_format == "sha1"
        && !refresh.project.default_branch.is_empty())
}

fn train_matches(
    query: &GitLabRefreshQuery,
    policy: &PolicyBinding,
    train: &GitLabTrainCar,
) -> Result<bool, ProviderError> {
    let active = match train.status.as_str() {
        "idle" | "fresh" => true,
        "stale" | "merging" | "merged" | "skip_merged" => false,
        _ => return Err(ProviderError::InvalidResponse),
    };
    let open = match train.merge_request_state.as_str() {
        "opened" => true,
        "closed" | "locked" | "merged" => false,
        _ => return Err(ProviderError::InvalidResponse),
    };
    let pipeline_running = match train.pipeline_status.as_str() {
        "running" => true,
        "canceled"
        | "created"
        | "failed"
        | "manual"
        | "pending"
        | "preparing"
        | "scheduled"
        | "skipped"
        | "success"
        | "waiting_for_callback"
        | "waiting_for_resource" => false,
        _ => return Err(ProviderError::InvalidResponse),
    };
    Ok(active
        && open
        && pipeline_running
        && train.id > 0
        && train.target_branch == policy.target_branch
        && train.merge_request_iid == query.merge_request_iid
        && train.merge_request_project_id == query.project_id
        && train.pipeline_id == query.pipeline_id
        && train.pipeline_project_id == query.project_id
        && train.pipeline_sha == query.gate_commit.as_str()
        && train.pipeline_ref == train_ref(query.merge_request_iid)
        && train.pipeline_source == MERGE_REQUEST_PIPELINE)
}

fn policy_authorized(policy: &PolicyBinding, refresh: &GitLabRefresh) -> bool {
    let mut protections = refresh
        .protections
        .iter()
        .filter(|protection| wildcard_matches(&protection.name, &policy.target_branch))
        .peekable();
    let protected = protections.peek().is_some()
        && protections.all(|protection| {
            !protection.allow_force_push
                && !protection.push_access_levels.is_empty()
                && protection.push_access_levels.iter().all(|access| {
                    access.access_level == 0
                        && access.user_id.is_none()
                        && access.group_id.is_none()
                        && access.deploy_key_id.is_none()
                        && access.member_role_id.is_none()
                })
        });
    refresh.project.checks.pipeline_must_succeed
        && !refresh.project.checks.skipped_pipeline_allowed
        && refresh.project.checks.merged_results_enabled
        && refresh.project.train.enabled
        && !refresh.project.train.skip_allowed
        && refresh.project.train.enforcement == TRAIN_ENFORCEMENT
        && refresh.project.merge_method == "merge"
        && refresh.project.squash_option == "never"
        && !refresh.merge_request.squash_on_merge
        && protected
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let starts_anywhere = pattern.starts_with('*');
    let ends_anywhere = pattern.ends_with('*');
    let mut parts = pattern
        .split('*')
        .filter(|part| !part.is_empty())
        .peekable();
    let mut remaining = value;
    let mut first = true;
    while let Some(part) = parts.next() {
        if first && !starts_anywhere {
            let Some(rest) = remaining.strip_prefix(part) else {
                return false;
            };
            remaining = rest;
        } else if parts.peek().is_none() && !ends_anywhere {
            return remaining.ends_with(part);
        } else {
            let Some(position) = remaining.find(part) else {
                return false;
            };
            let Some(rest) = remaining.get(position.saturating_add(part.len())..) else {
                return false;
            };
            remaining = rest;
        }
        first = false;
    }
    if pattern.is_empty() {
        value.is_empty()
    } else {
        starts_anywhere || ends_anywhere || remaining.is_empty()
    }
}

fn exact_oid(raw: &str) -> Result<Oid, ProviderError> {
    exact_sha1(raw).ok_or(ProviderError::InvalidResponse)
}
