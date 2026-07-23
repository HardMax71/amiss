use amiss_controller::{
    ChangeSnapshot, ChangeState, OidPair, ProviderError, Publication, RunIdentity, RunRefs,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use crate::GiteaPullRequest;
use crate::identity::parse_change_id;

use super::Config;
use super::model::{
    BranchProtectionRecord, PullRepositoryRecord, PullRequestRecord, RefreshData, RepositoryRecord,
    ReviewRecord, UserRecord,
};

pub(super) fn validate_request(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
) -> Result<(), ProviderError> {
    valid_response(
        request_route_matches(config, pull_request)
            && request_subject_matches(config, pull_request),
    )
}

fn request_route_matches(config: &Config, pull_request: GiteaPullRequest<'_>) -> bool {
    pull_request.reviewer_id == config.reviewer.id
        && pull_request.repository_id > 0
        && pull_request.pull_request_id > 0
        && pull_request.number > 0
        && pull_request.change.provider == config.provider
}

fn request_subject_matches(config: &Config, pull_request: GiteaPullRequest<'_>) -> bool {
    let repository = &pull_request.change.repository;
    RepositoryIdentity::new(
        repository.host.clone(),
        repository.owner.clone(),
        repository.name.clone(),
    )
    .as_ref()
        == Some(repository)
        && repository.host == config.provider.instance.as_str()
        && repository.owner == pull_request.repository_owner
        && repository.name == pull_request.repository_name
        && !repository.owner.contains('/')
        && exact_oid(pull_request.candidate_commit.as_str()).as_ref()
            == Ok(pull_request.candidate_commit)
        && parse_change_id(pull_request.change.change.as_str())
            == Some((
                pull_request.repository_id,
                pull_request.pull_request_id,
                pull_request.number,
            ))
}

pub(super) fn snapshot(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
    data: &RefreshData,
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

    let candidate = exact_oid(&data.candidate.sha)?;
    let current_head = exact_oid(&data.pull_request.head.sha)?;
    let fetched_head = exact_oid(&data.current_head.sha)?;
    let base = exact_oid(&data.target.sha)?;
    let branch_base = data
        .target_branch
        .commit
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)
        .and_then(|commit| exact_oid(&commit.id))?;
    let candidate_tree = commit_tree(&data.candidate)?;
    let current_head_tree = commit_tree(&data.current_head)?;
    let base_tree = commit_tree(&data.target)?;
    let merge_base = exact_oid(&data.pull_request.merge_base)?;
    if candidate != *pull_request.candidate_commit
        || current_head != fetched_head
        || data.pull_request.base.sha != data.target.sha
        || base != branch_base
        || current_head_tree != candidate_tree && current_head == candidate
    {
        return Err(ProviderError::InvalidResponse);
    }

    let open = match data.pull_request.state.as_str() {
        "open" if !data.pull_request.merged => true,
        "closed" => false,
        _ => return Err(ProviderError::InvalidResponse),
    };
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
    let exact_head = current_head == *pull_request.candidate_commit;
    let up_to_date = merge_base == base;
    let state = if !exact_head {
        ChangeState::Superseded
    } else if !authorized {
        ChangeState::AuthorizationRevoked
    } else if !open {
        ChangeState::Closed
    } else if !up_to_date || !data.pull_request.mergeable {
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
) -> Result<ChangeState, ProviderError> {
    let fresh = snapshot(config, pull_request, data)?;
    let exact = fresh.run == publication.run
        && fresh.gate_commit == publication.gate_commit
        && fresh.gate_commit == *pull_request.candidate_commit;
    Ok(if exact {
        fresh.state
    } else {
        ChangeState::Superseded
    })
}

fn validate_reviewer(config: &Config, reviewer: &UserRecord) -> Result<(), ProviderError> {
    (reviewer.id == config.reviewer.id
        && reviewer.login.eq_ignore_ascii_case(&config.reviewer.login))
    .then_some(())
    .ok_or(ProviderError::AuthorizationRevoked)
}

fn validate_change(
    config: &Config,
    pull_request: GiteaPullRequest<'_>,
    repository: &RepositoryRecord,
    authoritative: &PullRequestRecord,
) -> Result<(), ProviderError> {
    let repository_identity = repository_identity(
        config,
        &repository.owner.login,
        &repository.name,
        &repository.full_name,
    )?;
    let base_repository = authoritative
        .base
        .repo
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?;
    let base_identity = pull_repository_identity(config, base_repository)?;
    let head_identity = authoritative
        .head
        .repo
        .as_ref()
        .map(|head| pull_repository_identity(config, head))
        .transpose()?;
    valid_response(
        repository.id == pull_request.repository_id
            && repository_identity == pull_request.change.repository
            && repository.object_format_name == "sha1"
            && base_repository.id == pull_request.repository_id
            && base_identity == pull_request.change.repository
            && authoritative.id == pull_request.pull_request_id
            && authoritative.number == pull_request.number
            && authoritative.base.repo_id == pull_request.repository_id
            && authoritative.head.repo_id > 0
            && (head_identity.is_some() || authoritative.state == "closed"),
    )
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
            || exact_oid(&review.commit_id).is_err()
            || !matches!(
                review.state.as_str(),
                "APPROVED" | "PENDING" | "COMMENT" | "REQUEST_CHANGES" | "REQUEST_REVIEW"
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
    let exact_reviewer = protection
        .approvals
        .approvals_whitelist_usernames
        .as_slice()
        == [config.reviewer.login.as_str()]
        || protection.approvals.approvals_whitelist_usernames.len() == 1
            && protection
                .approvals
                .approvals_whitelist_usernames
                .first()
                .is_some_and(|login| login.eq_ignore_ascii_case(&config.reviewer.login));
    let admin_enforced = matches!(
        (
            protection.overrides.block_admin_merge_override,
            protection.overrides.apply_to_admins,
        ),
        (Some(true), None) | (None, Some(true))
    );
    let gitea_shape = protection.overrides.block_admin_merge_override == Some(true)
        && protection.overrides.apply_to_admins.is_none()
        && repository.allow_manual_merge == Some(false)
        && gitea_extensions_closed(&protection.force, &protection.bypass);
    let forgejo_shape = protection.overrides.block_admin_merge_override.is_none()
        && protection.overrides.apply_to_admins == Some(true)
        && repository.allow_manual_merge.is_none()
        && forgejo_extensions_absent(&protection.force, &protection.bypass);
    branch.name == pull_request.base.branch
        && branch.protected
        && branch.required_approvals == 1
        && !branch.effective_branch_protection_name.is_empty()
        && branch.effective_branch_protection_name == protection.rule_name
        && direct_push_closed(&protection.writes)
        && (gitea_shape || forgejo_shape)
        && protection.approvals.required_approvals == 1
        && protection.approvals.enable_approvals_whitelist
        && exact_reviewer
        && protection.approvals.approvals_whitelist_teams.is_empty()
        && protection.reviews.block_on_rejected_reviews
        && protection.reviews.block_on_outdated_branch
        && protection.reviews.dismiss_stale_approvals
        && !protection.overrides.ignore_stale_approvals
        && admin_enforced
}

fn direct_push_closed(protection: &super::model::WriteProtection) -> bool {
    !protection.enable_push
        && !protection.enable_push_whitelist
        && protection.push_whitelist_usernames.is_empty()
        && protection.push_whitelist_teams.is_empty()
        && !protection.push_whitelist_deploy_keys
        && protection.unprotected_file_patterns.is_empty()
}

fn gitea_extensions_closed(
    force: &super::model::ForceProtection,
    bypass: &super::model::BypassProtection,
) -> bool {
    extensions_satisfy(
        force,
        bypass,
        |value| value == Some(false),
        |value| value.is_some_and(<[String]>::is_empty),
    )
}

fn forgejo_extensions_absent(
    force: &super::model::ForceProtection,
    bypass: &super::model::BypassProtection,
) -> bool {
    extensions_satisfy(force, bypass, absent_flag, absent_allowlist)
}

fn absent_flag(value: Option<bool>) -> bool {
    value.is_none()
}

fn absent_allowlist(value: Option<&[String]>) -> bool {
    value.is_none()
}

fn extensions_satisfy(
    force: &super::model::ForceProtection,
    bypass: &super::model::BypassProtection,
    flag: impl Fn(Option<bool>) -> bool,
    allowlist: impl Fn(Option<&[String]>) -> bool,
) -> bool {
    [
        force.enable_force_push,
        force.enable_force_push_allowlist,
        force.force_push_allowlist_deploy_keys,
        bypass.enable_bypass_allowlist,
    ]
    .into_iter()
    .all(flag)
        && [
            force.force_push_allowlist_usernames.as_deref(),
            force.force_push_allowlist_teams.as_deref(),
            bypass.bypass_allowlist_usernames.as_deref(),
            bypass.bypass_allowlist_teams.as_deref(),
        ]
        .into_iter()
        .all(allowlist)
}

fn repository_identity(
    config: &Config,
    owner: &str,
    name: &str,
    full_name: &str,
) -> Result<RepositoryIdentity, ProviderError> {
    let owner = owner.to_ascii_lowercase();
    let name = name.to_ascii_lowercase();
    if !full_name.eq_ignore_ascii_case(&format!("{owner}/{name}")) {
        return Err(ProviderError::InvalidResponse);
    }
    RepositoryIdentity::new(config.provider.instance.as_str().to_owned(), owner, name)
        .ok_or(ProviderError::InvalidResponse)
}

fn pull_repository_identity(
    config: &Config,
    repository: &PullRepositoryRecord,
) -> Result<RepositoryIdentity, ProviderError> {
    (repository.id > 0)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)?;
    repository_identity(
        config,
        &repository.owner.login,
        &repository.name,
        &repository.full_name,
    )
}

fn commit_tree(commit: &super::model::CommitRecord) -> Result<Oid, ProviderError> {
    commit
        .commit
        .as_ref()
        .and_then(|commit| commit.tree.as_ref())
        .ok_or(ProviderError::InvalidResponse)
        .and_then(|tree| exact_oid(&tree.sha))
}

fn exact_oid(raw: &str) -> Result<Oid, ProviderError> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).ok_or(ProviderError::InvalidResponse)
}

fn valid_response(valid: bool) -> Result<(), ProviderError> {
    valid.then_some(()).ok_or(ProviderError::InvalidResponse)
}

fn branch_ref(branch: &str) -> Result<BranchRef, ProviderError> {
    BranchRef::new(format!("refs/heads/{branch}")).ok_or(ProviderError::InvalidResponse)
}
