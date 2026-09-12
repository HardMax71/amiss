use amiss_controller::{
    ChangeSnapshot, ChangeState, OidPair, ProviderError, Publication, RunIdentity, RunRefs,
};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use crate::GitHubPullRequest;
use crate::pull::State;

use super::Config;
use super::model::{PullRequestRecord, RefreshData, RepositoryRecord, WorkflowRepositoryRecord};
use super::rules::BranchRule;

pub(super) fn validate_request(
    config: &Config,
    pull_request: GitHubPullRequest<'_>,
) -> Result<(), ProviderError> {
    let repository = &pull_request.change.repository;
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
        || repository.host() != config.provider.instance.as_str()
        || repository.owner() != pull_request.repository_owner
        || repository.name() != pull_request.repository_name
        || !crate::acquisition::canonical_github_repository(repository)
        || !exact_change
        || pull_request.candidate_commit.object_format() != ObjectFormat::Sha1
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
    let authorized = rules_authorize(config, &data.rules);

    let candidate = &data.candidate.sha;
    let current_head = &data.pull_request.head.sha;
    let base = &data.target.sha;
    if [
        candidate,
        current_head,
        &data.current_head.sha,
        base,
        &data.candidate.tree,
        &data.current_head.tree,
        &data.target.tree,
        &data.gate.sha,
    ]
    .into_iter()
    .any(|oid| oid.object_format() != ObjectFormat::Sha1)
        || candidate != pull_request.candidate_commit
        || current_head != &data.current_head.sha
        || data.pull_request.base.sha != data.target.sha
    {
        return Err(ProviderError::InvalidResponse);
    }
    let gate_ready = gate_ready(data, base, current_head, &data.current_head.tree)?;

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
        OidPair {
            base: base.clone(),
            candidate: candidate.clone(),
        },
        OidPair {
            base: data.target.tree.clone(),
            candidate: data.candidate.tree.clone(),
        },
    )
    .ok_or(ProviderError::InvalidResponse)?;
    let state = if current_head != pull_request.candidate_commit {
        ChangeState::Superseded
    } else if !authorized {
        ChangeState::AuthorizationRevoked
    } else if data.pull_request.state == State::Closed {
        ChangeState::Closed
    } else if gate_ready {
        ChangeState::Active
    } else {
        ChangeState::Superseded
    };
    Ok(ChangeSnapshot {
        state,
        run,
        gate_commit: data.gate.sha.clone(),
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
    if [&authoritative.head.sha, &authoritative.base.sha]
        .into_iter()
        .chain(authoritative.merge_commit_sha.as_ref())
        .any(|oid| oid.object_format() != ObjectFormat::Sha1)
    {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(authoritative.head.sha == *pull_request.candidate_commit
        && authoritative.base.sha == publication.run.commits.base
        && branch_ref(&authoritative.head.branch)? == publication.run.refs.candidate
        && branch_ref(&authoritative.base.branch)? == publication.run.refs.target
        && authoritative.merge_commit_sha.as_ref() == Some(&publication.gate_commit))
}

fn gate_ready(
    data: &RefreshData,
    base: &Oid,
    candidate: &Oid,
    candidate_tree: &Oid,
) -> Result<bool, ProviderError> {
    if data.pull_request.merge_commit_sha.as_ref() != Some(&data.gate.sha) {
        return Err(ProviderError::InvalidResponse);
    }
    if data.pull_request.state == State::Closed {
        return Ok(false);
    }
    match data.pull_request.mergeable {
        None => Err(ProviderError::Unavailable),
        Some(false) => Ok(false),
        Some(true) => {
            let [gate_base, gate_candidate] = data.gate.parents.as_slice() else {
                return Err(ProviderError::InvalidResponse);
            };
            let parents_match = gate_base == base && gate_candidate == candidate;
            if !parents_match {
                return Err(ProviderError::InvalidResponse);
            }
            if data.gate.tree.object_format() != ObjectFormat::Sha1 {
                return Err(ProviderError::InvalidResponse);
            }
            Ok(&data.gate.tree == candidate_tree)
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
        None if authoritative.state == State::Closed => Ok(()),
        None => Err(ProviderError::InvalidResponse),
    }
}

fn rules_authorize(config: &Config, rules: &[BranchRule]) -> bool {
    let mut found = false;
    let mut bound = true;
    for rule in rules {
        let BranchRule::RequiredStatusChecks(rule) = rule else {
            continue;
        };
        let parameters = &rule.parameters;
        for required in parameters
            .required_status_checks
            .iter()
            .filter(|required| required.context == config.required_status_name)
        {
            if required.integration_id.map(u64::from) != Some(config.app_id)
                || !parameters.strict_required_status_checks_policy
            {
                bound = false;
            }
            found = true;
        }
    }
    found && bound
}

pub(super) fn repository_identity(
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
    repository: &WorkflowRepositoryRecord,
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

fn branch_ref(branch: &str) -> Result<BranchRef, ProviderError> {
    BranchRef::new(format!("refs/heads/{branch}")).ok_or(ProviderError::InvalidResponse)
}
