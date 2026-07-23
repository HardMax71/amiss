use amiss_controller::{
    ChangeSnapshot, ChangeState, OidPair, ProviderError, Publication, RunIdentity, RunRefs,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use crate::GitHubPullRequest;

use super::Config;
use super::model::{
    BranchRule, PullRepositoryRecord, PullRequestRecord, RefreshData, RepositoryRecord,
    RequiredStatusParameters,
};

const REQUIRED_STATUS_RULE: &str = "required_status_checks";

pub(super) fn validate_request(
    config: &Config,
    pull_request: GitHubPullRequest<'_>,
) -> Result<(), ProviderError> {
    let repository = &pull_request.change.repository;
    let exact_candidate = Oid::new(
        ObjectFormat::Sha1,
        pull_request.candidate_commit.as_str().to_owned(),
    )
    .as_ref()
        == Some(pull_request.candidate_commit);
    let canonical_repository = RepositoryIdentity::new(
        repository.host.clone(),
        repository.owner.clone(),
        repository.name.clone(),
    )
    .as_ref()
        == Some(repository);
    let exact_change = crate::parse_change_id(pull_request.change.change.as_str())
        == Some((
            pull_request.repository_id,
            pull_request.pull_request_id,
            pull_request.number,
        ));
    if pull_request.installation_id != config.installation_id
        || pull_request.repository_id == 0
        || pull_request.pull_request_id == 0
        || pull_request.number == 0
        || pull_request.change.provider != config.provider
        || repository.host != config.provider.instance.as_str()
        || repository.owner != pull_request.repository_owner
        || repository.name != pull_request.repository_name
        || repository.owner.contains('/')
        || !canonical_repository
        || !exact_change
        || !exact_candidate
    {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(())
}

pub(super) fn snapshot(
    config: &Config,
    pull_request: GitHubPullRequest<'_>,
    data: &RefreshData,
) -> Result<ChangeSnapshot, ProviderError> {
    validate_request(config, pull_request)?;
    validate_repository(config, pull_request, &data.repository)?;
    validate_pull_request(config, pull_request, &data.pull_request)?;
    let authorized = rules_authorize(config, &data.rules)?;

    let candidate = exact_oid(&data.candidate.sha)?;
    let current_head = exact_oid(&data.pull_request.head.sha)?;
    let fetched_head = exact_oid(&data.current_head.sha)?;
    let base = exact_oid(&data.target.sha)?;
    let candidate_tree = exact_oid(&data.candidate.tree)?;
    let current_head_tree = exact_oid(&data.current_head.tree)?;
    let base_tree = exact_oid(&data.target.tree)?;
    let gate_commit = exact_oid(&data.gate.sha)?;
    if candidate != *pull_request.candidate_commit
        || current_head != fetched_head
        || data.pull_request.base.sha != data.target.sha
    {
        return Err(ProviderError::InvalidResponse);
    }
    let open = match data.pull_request.state.as_str() {
        "open" => true,
        "closed" => false,
        _ => return Err(ProviderError::InvalidResponse),
    };
    let gate_ready = gate_ready(data, open, &base, &current_head, &current_head_tree)?;

    let refs = RunRefs {
        forge: ForgeDialect::Github,
        candidate: branch_ref(&data.pull_request.head.branch)?,
        target: branch_ref(&data.pull_request.base.branch)?,
        default_branch: branch_ref(&data.repository.default_branch)?,
    };
    let run = RunIdentity::new(
        pull_request.change.clone(),
        refs,
        ObjectFormat::Sha1,
        OidPair { base, candidate },
        OidPair {
            base: base_tree,
            candidate: candidate_tree,
        },
    )
    .ok_or(ProviderError::InvalidResponse)?;
    let state = if current_head != *pull_request.candidate_commit {
        ChangeState::Superseded
    } else if !authorized {
        ChangeState::AuthorizationRevoked
    } else if !open {
        ChangeState::Closed
    } else if gate_ready {
        ChangeState::Active
    } else {
        ChangeState::Superseded
    };
    Ok(ChangeSnapshot {
        state,
        run,
        gate_commit,
    })
}

pub(super) fn publication_target_is_current(
    config: &Config,
    pull_request: GitHubPullRequest<'_>,
    publication: &Publication,
    authoritative: &PullRequestRecord,
) -> Result<bool, ProviderError> {
    validate_request(config, pull_request)?;
    validate_pull_request(config, pull_request, authoritative)?;
    match authoritative.state.as_str() {
        "open" | "closed" => {}
        _ => return Err(ProviderError::InvalidResponse),
    }
    let gate = authoritative
        .merge_commit_sha
        .as_deref()
        .map(exact_oid)
        .transpose()?;
    Ok(
        exact_oid(&authoritative.head.sha)? == *pull_request.candidate_commit
            && exact_oid(&authoritative.base.sha)? == publication.run.commits.base
            && branch_ref(&authoritative.head.branch)? == publication.run.refs.candidate
            && branch_ref(&authoritative.base.branch)? == publication.run.refs.target
            && gate.as_ref() == Some(&publication.gate_commit),
    )
}

fn gate_ready(
    data: &RefreshData,
    open: bool,
    base: &Oid,
    candidate: &Oid,
    candidate_tree: &Oid,
) -> Result<bool, ProviderError> {
    if data.pull_request.merge_commit_sha.as_deref() != Some(data.gate.sha.as_str()) {
        return Err(ProviderError::InvalidResponse);
    }
    if !open {
        return Ok(false);
    }
    match data.pull_request.mergeable {
        None => Err(ProviderError::Unavailable),
        Some(false) => Ok(false),
        Some(true) => {
            let [gate_base, gate_candidate] = data.gate.parents.as_slice() else {
                return Err(ProviderError::InvalidResponse);
            };
            let parents_match =
                exact_oid(gate_base)? == *base && exact_oid(gate_candidate)? == *candidate;
            if !parents_match {
                return Err(ProviderError::InvalidResponse);
            }
            Ok(exact_oid(&data.gate.tree)? == *candidate_tree)
        }
    }
}

fn validate_repository(
    config: &Config,
    pull_request: GitHubPullRequest<'_>,
    repository: &RepositoryRecord,
) -> Result<(), ProviderError> {
    let identity = repository_identity(
        config,
        &repository.owner.login,
        &repository.name,
        &repository.full_name,
    )?;
    (repository.id == pull_request.repository_id && identity == pull_request.change.repository)
        .then_some(())
        .ok_or(ProviderError::InvalidResponse)
}

fn validate_pull_request(
    config: &Config,
    pull_request: GitHubPullRequest<'_>,
    authoritative: &PullRequestRecord,
) -> Result<(), ProviderError> {
    let base_repository = authoritative
        .base
        .repo
        .as_ref()
        .ok_or(ProviderError::InvalidResponse)?;
    let base_identity = pull_repository_identity(config, base_repository)?;
    if authoritative.id != pull_request.pull_request_id
        || authoritative.number != pull_request.number
        || base_repository.id != pull_request.repository_id
        || base_identity != pull_request.change.repository
    {
        return Err(ProviderError::InvalidResponse);
    }
    match authoritative.head.repo.as_ref() {
        Some(repository) => {
            pull_repository_identity(config, repository)?;
            Ok(())
        }
        None if authoritative.state == "closed" => Ok(()),
        None => Err(ProviderError::InvalidResponse),
    }
}

fn rules_authorize(config: &Config, rules: &[BranchRule]) -> Result<bool, ProviderError> {
    let mut found = false;
    let mut bound = true;
    for rule in rules
        .iter()
        .filter(|rule| rule.kind == REQUIRED_STATUS_RULE)
    {
        let parameters: RequiredStatusParameters = rule
            .parameters
            .clone()
            .ok_or(ProviderError::InvalidResponse)
            .and_then(|value| {
                serde_json::from_value(value).map_err(|_defect| ProviderError::InvalidResponse)
            })?;
        for required in parameters
            .required_status_checks
            .iter()
            .filter(|required| required.context == config.required_status_name)
        {
            if required.integration_id != Some(config.app_id)
                || !parameters.strict_required_status_checks_policy
            {
                bound = false;
            }
            found = true;
        }
    }
    Ok(found && bound)
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

fn exact_oid(raw: &str) -> Result<Oid, ProviderError> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).ok_or(ProviderError::InvalidResponse)
}

fn branch_ref(branch: &str) -> Result<BranchRef, ProviderError> {
    BranchRef::new(format!("refs/heads/{branch}")).ok_or(ProviderError::InvalidResponse)
}
