use amiss_controller::{
    ChangeSnapshot, ChangeState, OidPair, ProviderError, Publication, RunIdentity, RunRefs,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, RepositoryIdentity};

use crate::identity::parse_change_id;
use crate::issue::IssueState;
use crate::{GiteaObjects, GiteaPullRequest};

use super::Config;
use super::model::{
    BranchProtectionRecord, PullRequestRecord, RefreshData, RepositoryRecord, ReviewRecord,
    UserRecord,
};

pub(super) fn validate_request(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
) -> Result<(), ProviderError> {
    let repository = &pull_request.change.repository;
    (pull_request.reviewer_id == config.reviewer.id
        && pull_request.repository_id > 0
        && pull_request.pull_request_id > 0
        && pull_request.number > 0
        && pull_request.change.provider == config.provider
        && RepositoryIdentity::new(
            repository.host().to_owned(),
            repository.owner().to_owned(),
            repository.name().to_owned(),
        )
        .as_ref()
            == Some(repository)
        && repository.host() == config.provider.instance.as_str()
        && repository.owner() == pull_request.repository_owner
        && repository.name() == pull_request.repository_name
        && !repository.owner().contains('/')
        && pull_request.candidate_commit.object_format() == ObjectFormat::Sha1
        && parse_change_id(pull_request.change.change.as_str())
            == Some((
                pull_request.repository_id,
                pull_request.pull_request_id,
                pull_request.number,
            )))
    .then_some(())
    .ok_or(ProviderError::InvalidResponse)
}

pub(super) fn snapshot(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
    data: &RefreshData,
    objects: &GiteaObjects,
) -> Result<ChangeSnapshot, ProviderError> {
    validate_request(config, pull_request)?;
    validate_reviewer(config, &data.reviewer)?;
    validate_change(config, pull_request, &data.repository, &data.pull_request)?;
    validate_reviews(config, &data.reviews)?;
    let authorized = protection_authorizes(
        config,
        &data.repository,
        &data.pull_request,
        &data.target_branch,
        &data.protection,
    );

    let candidate = data.candidate.sha.clone();
    let current_head = data
        .pull_request
        .head
        .sha
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?;
    let fetched_head = &data.current_head.sha;
    let base = data.target.sha.clone();
    let branch_base = &data
        .target_branch
        .commit
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?
        .id;
    let candidate_tree = objects.candidate.tree.clone();
    let base_object = objects
        .base
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?;
    let base_tree = base_object.tree.clone();
    let merge_base = data
        .pull_request
        .merge_base
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?;
    if candidate != *pull_request.candidate_commit
        || current_head != fetched_head
        || current_head.object_format() != ObjectFormat::Sha1
        || merge_base.object_format() != ObjectFormat::Sha1
        || data.pull_request.base.sha.as_ref() != Some(&data.target.sha)
        || base != *branch_base
        || objects.candidate.id != candidate
        || base_object.id != base
    {
        return Err(ProviderError::InvalidResponse);
    }

    let open = data.pull_request.state == IssueState::Open;
    if open && data.pull_request.merged {
        return Err(ProviderError::InvalidResponse);
    }
    let refs = RunRefs {
        forge: ForgeDialect::Gitea,
        candidate: branch_ref(&data.pull_request.head.branch)?,
        target: branch_ref(&data.pull_request.base.branch)?,
        default_branch: branch_ref(&data.repository.default_branch)?,
    };
    let run = RunIdentity::new(
        pull_request.change.clone(),
        refs,
        ObjectFormat::Sha1,
        OidPair {
            base: base.clone(),
            candidate: candidate.clone(),
        },
        OidPair {
            base: base_tree,
            candidate: candidate_tree,
        },
    )
    .ok_or(ProviderError::InvalidResponse)?;
    let exact_head = current_head == pull_request.candidate_commit;
    let up_to_date = *merge_base == base;
    let state = if !exact_head {
        ChangeState::Superseded
    } else if !authorized {
        ChangeState::AuthorizationRevoked
    } else if !open {
        ChangeState::Closed
    } else if !data.pull_request.mergeable {
        return Err(ProviderError::Unavailable);
    } else if !up_to_date {
        ChangeState::Superseded
    } else {
        ChangeState::Active
    };
    Ok(ChangeSnapshot {
        state,
        run,
        gate_commit: candidate,
    })
}

pub(super) fn publication_target_is_current(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
    publication: &Publication,
    data: &RefreshData,
    objects: &GiteaObjects,
) -> Result<ChangeState, ProviderError> {
    let fresh = snapshot(config, pull_request, data, objects)?;
    let exact = fresh.run == publication.run
        && fresh.gate_commit == publication.gate_commit
        && fresh.gate_commit == *pull_request.candidate_commit;
    Ok(if exact {
        fresh.state
    } else {
        ChangeState::Superseded
    })
}

pub(super) fn validate_reviewer(
    config: &Config,
    reviewer: &UserRecord,
) -> Result<(), ProviderError> {
    if reviewer.id != config.reviewer.id
        || !reviewer.login.eq_ignore_ascii_case(&config.reviewer.login)
    {
        return Err(ProviderError::AuthorizationRevoked);
    }
    Ok(())
}

fn validate_change(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
    repository: &RepositoryRecord,
    authoritative: &PullRequestRecord,
) -> Result<(), ProviderError> {
    let host = config.provider.instance.as_str();
    let identity = repository_identity(host, repository)?;
    let base_repository = authoritative
        .base
        .repo
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?;
    let base_identity = repository_identity(host, base_repository)?;
    let head_identity = authoritative
        .head
        .repo
        .as_ref()
        .map(|head| repository_identity(host, head))
        .transpose()?;
    (repository.id == pull_request.repository_id
        && identity == pull_request.change.repository
        && repository.object_format_name == ObjectFormat::Sha1
        && base_repository.id == pull_request.repository_id
        && base_identity == pull_request.change.repository
        && authoritative.id == pull_request.pull_request_id
        && authoritative.number == pull_request.number
        && u64::try_from(authoritative.base.repo_id) == Ok(pull_request.repository_id)
        && authoritative.head.repo_id > 0
        && (head_identity.is_some() || authoritative.state == IssueState::Closed))
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)
}

fn validate_reviews(config: &Config, reviews: &[ReviewRecord]) -> Result<(), ProviderError> {
    for review in reviews {
        let Some(reviewer) = review.user.as_ref() else {
            continue;
        };
        let own_id = reviewer.id == config.reviewer.id;
        let own_login = reviewer.login.eq_ignore_ascii_case(&config.reviewer.login);
        if !own_id && !own_login {
            continue;
        }
        if !own_id
            || !own_login
            || review.id == 0
            || !review.commit_id.as_ref().map_or(
                review.state == crate::review::ReviewState::RequestReview,
                |commit| commit.object_format() == ObjectFormat::Sha1,
            )
        {
            return Err(ProviderError::InvalidResponse);
        }
    }
    Ok(())
}

fn protection_authorizes(
    config: &Config,
    repository: &RepositoryRecord,
    pull_request: &PullRequestRecord,
    branch: &super::model::BranchRecord,
    protection: &BranchProtectionRecord,
) -> bool {
    let flags = [
        protection.enable_force_push,
        protection.enable_force_push_allowlist,
        protection.force_push_allowlist_deploy_keys,
        protection.enable_bypass_allowlist,
    ];
    let allowlists = [
        protection.force_push_allowlist_usernames.as_deref(),
        protection.force_push_allowlist_teams.as_deref(),
        protection.bypass_allowlist_usernames.as_deref(),
        protection.bypass_allowlist_teams.as_deref(),
    ];
    let exact_reviewer = protection.approvals_whitelist_usernames.len() == 1
        && protection
            .approvals_whitelist_usernames
            .first()
            .is_some_and(|login| login.eq_ignore_ascii_case(&config.reviewer.login));
    let gitea_shape = protection.block_admin_merge_override == Some(true)
        && protection.apply_to_admins.is_none()
        && protection.priority.is_some()
        && protection.block_on_codeowner_reviews.is_some()
        && repository.allow_manual_merge == Some(false)
        && flags.iter().all(|value| *value == Some(false))
        && allowlists
            .iter()
            .all(|value| value.is_some_and(<[String]>::is_empty));
    let forgejo_shape = protection.block_admin_merge_override.is_none()
        && protection.apply_to_admins == Some(true)
        && protection.priority.is_none()
        && protection.block_on_codeowner_reviews.is_none()
        && repository.allow_manual_merge.is_none()
        && flags.iter().all(Option::is_none)
        && allowlists.iter().all(Option::is_none);
    branch.name == pull_request.base.branch
        && branch.protected
        && branch.required_approvals == 1
        && !branch.effective_branch_protection_name.is_empty()
        && branch.effective_branch_protection_name == protection.rule_name
        && !protection.enable_push
        && !protection.enable_push_whitelist
        && protection.push_whitelist_usernames.is_empty()
        && protection.push_whitelist_teams.is_empty()
        && !protection.push_whitelist_deploy_keys
        && protection.unprotected_file_patterns.is_empty()
        && (gitea_shape || forgejo_shape)
        && protection.required_approvals == 1
        && protection.enable_approvals_whitelist
        && exact_reviewer
        && protection.approvals_whitelist_teams.is_empty()
        && protection.block_on_rejected_reviews
        && protection.block_on_outdated_branch
        && protection.dismiss_stale_approvals
        && !protection.ignore_stale_approvals
}

pub(super) fn repository_identity(
    host: &str,
    repository: &RepositoryRecord,
) -> Result<RepositoryIdentity, ProviderError> {
    let owner = repository.owner.login.to_ascii_lowercase();
    let name = repository.name.to_ascii_lowercase();
    if repository.id == 0
        || !repository
            .full_name
            .eq_ignore_ascii_case(&format!("{owner}/{name}"))
    {
        return Err(ProviderError::InvalidResponse);
    }
    RepositoryIdentity::new(host.to_owned(), owner, name).ok_or(ProviderError::InvalidResponse)
}

fn branch_ref(branch: &str) -> Result<BranchRef, ProviderError> {
    BranchRef::new(format!("refs/heads/{branch}")).ok_or(ProviderError::InvalidResponse)
}
