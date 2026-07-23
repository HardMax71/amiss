use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use secrecy::{SecretSlice, SecretString};
use serde::Serialize;
use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_wire::model::Oid;

use crate::GitHubPullRequest;

use super::model::{
    BranchRule, CheckRunPage, CheckRunRecord, CommitRecord, CreateCheckRun, GateCommitRecord,
    GitCommitRecord, PullRequestRecord, RefreshData, RepositoryCommitRecord, RepositoryRecord,
};
use super::{GitHubClientError, GitHubTimeouts};

mod transport;

use self::transport::Transport;

const PAGE_SIZE: usize = 100;
const PAGE_SIZE_U8: u8 = 100;
const MAX_PAGES: u32 = 10;

#[derive(Clone, Copy)]
pub(super) struct OperationDeadline(Instant);

impl OperationDeadline {
    pub(super) fn after(timeout: Duration) -> Result<Self, ProviderError> {
        Instant::now()
            .checked_add(timeout)
            .map(Self)
            .ok_or(ProviderError::Unavailable)
    }

    fn remaining(self) -> Result<Duration, ProviderError> {
        let remaining = self.0.saturating_duration_since(Instant::now());
        (!remaining.is_zero())
            .then_some(remaining)
            .ok_or(ProviderError::Unavailable)
    }
}

pub(super) trait GitHubRest: Send + Sync {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError>;

    fn pull_request(
        &self,
        pull_request: GitHubPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<PullRequestRecord, ProviderError>;

    fn refresh_data(
        &self,
        pull_request: GitHubPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError>;

    fn check_runs(
        &self,
        pull_request: GitHubPullRequest<'_>,
        head_sha: &Oid,
        app_id: u64,
        name: &str,
        deadline: OperationDeadline,
    ) -> Result<Vec<CheckRunRecord>, ProviderError>;

    fn create_check_run(
        &self,
        pull_request: GitHubPullRequest<'_>,
        check: &CreateCheckRun,
        deadline: OperationDeadline,
    ) -> Result<CheckRunRecord, ProviderError>;
}

pub(super) struct HttpRest {
    transport: Transport,
}

impl HttpRest {
    pub(super) fn new(
        app_id: u64,
        installation_id: u64,
        private_key: SecretSlice<u8>,
        provider_instance: &str,
        api_base: &str,
        timeouts: GitHubTimeouts,
    ) -> Result<Self, GitHubClientError> {
        Ok(Self {
            transport: Transport::new(
                app_id,
                installation_id,
                private_key,
                provider_instance,
                api_base,
                timeouts,
            )?,
        })
    }

    pub(super) fn installation_access_token(&self) -> Result<SecretString, ProviderError> {
        self.transport.installation_access_token()
    }

    fn branch_rules(
        &self,
        owner: &str,
        name: &str,
        branch: &str,
        deadline: OperationDeadline,
    ) -> Result<Vec<BranchRule>, ProviderError> {
        let branch = path_segment(branch);
        let route = format!("/repos/{owner}/{name}/rules/branches/{branch}");
        let mut rules = Vec::new();
        for page in 1..=MAX_PAGES {
            let query = PageQuery {
                per_page: PAGE_SIZE_U8,
                page,
            };
            let route = query_route(&route, &query)?;
            let batch: Vec<BranchRule> = self.transport.get(&route, deadline)?;
            if batch.len() > PAGE_SIZE {
                return Err(ProviderError::InvalidResponse);
            }
            let complete = batch.len() < PAGE_SIZE;
            rules.extend(batch);
            if complete {
                return Ok(rules);
            }
        }
        Err(ProviderError::InvalidResponse)
    }

    fn git_commit(
        &self,
        owner: &str,
        name: &str,
        oid: &str,
        deadline: OperationDeadline,
    ) -> Result<GitCommitRecord, ProviderError> {
        self.transport.get(
            &format!("/repos/{owner}/{name}/git/commits/{}", path_segment(oid)),
            deadline,
        )
    }
}

impl GitHubRest for HttpRest {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        self.transport.deadline()
    }

    fn pull_request(
        &self,
        pull_request: GitHubPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<PullRequestRecord, ProviderError> {
        self.transport.get(
            &format!(
                "/repos/{}/{}/pulls/{}",
                pull_request.repository_owner, pull_request.repository_name, pull_request.number
            ),
            deadline,
        )
    }

    fn refresh_data(
        &self,
        pull_request: GitHubPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError> {
        let owner = pull_request.repository_owner;
        let name = pull_request.repository_name;
        let repository: RepositoryRecord = self
            .transport
            .get(&format!("/repos/{owner}/{name}"), deadline)?;
        let authoritative = self.pull_request(pull_request, deadline)?;
        let target: RepositoryCommitRecord = self.transport.get(
            &format!(
                "/repos/{owner}/{name}/commits/{}",
                path_segment(&authoritative.base.sha)
            ),
            deadline,
        )?;
        let candidate = self.git_commit(
            owner,
            name,
            pull_request.candidate_commit.as_str(),
            deadline,
        )?;
        let current_head = if authoritative.head.sha == candidate.sha {
            CommitRecord {
                sha: candidate.sha.clone(),
                tree: candidate.tree.sha.clone(),
            }
        } else {
            let current = self.git_commit(owner, name, &authoritative.head.sha, deadline)?;
            CommitRecord {
                sha: current.sha,
                tree: current.tree.sha,
            }
        };
        let gate_sha = authoritative
            .merge_commit_sha
            .as_deref()
            .ok_or(ProviderError::Unavailable)?;
        let gate = self.git_commit(owner, name, gate_sha, deadline)?;
        let rules = self.branch_rules(owner, name, &authoritative.base.branch, deadline)?;
        Ok(RefreshData {
            repository,
            pull_request: authoritative,
            target: CommitRecord {
                sha: target.sha,
                tree: target.commit.tree.sha,
            },
            candidate: CommitRecord {
                sha: candidate.sha,
                tree: candidate.tree.sha,
            },
            current_head,
            gate: GateCommitRecord {
                sha: gate.sha,
                tree: gate.tree.sha,
                parents: gate.parents.into_iter().map(|parent| parent.sha).collect(),
            },
            rules,
        })
    }

    fn check_runs(
        &self,
        pull_request: GitHubPullRequest<'_>,
        head_sha: &Oid,
        app_id: u64,
        name: &str,
        deadline: OperationDeadline,
    ) -> Result<Vec<CheckRunRecord>, ProviderError> {
        let route = format!(
            "/repos/{}/{}/commits/{}/check-runs",
            pull_request.repository_owner,
            pull_request.repository_name,
            head_sha.as_str(),
        );
        let mut runs = Vec::new();
        for page in 1..=MAX_PAGES {
            let query = CheckRunQuery {
                check_name: name.to_owned(),
                filter: "all",
                per_page: PAGE_SIZE_U8,
                page,
                app_id,
            };
            let route = query_route(&route, &query)?;
            let response: CheckRunPage = self.transport.get(&route, deadline)?;
            let count =
                u64::try_from(runs.len()).map_err(|_defect| ProviderError::InvalidResponse)?;
            if response.check_runs.len() > PAGE_SIZE
                || response.total_count < count
                || response.total_count > u64::from(PAGE_SIZE_U8) * u64::from(MAX_PAGES)
            {
                return Err(ProviderError::InvalidResponse);
            }
            runs.extend(response.check_runs);
            let count =
                u64::try_from(runs.len()).map_err(|_defect| ProviderError::InvalidResponse)?;
            if count == response.total_count {
                return Ok(runs);
            }
            if count > response.total_count {
                return Err(ProviderError::InvalidResponse);
            }
        }
        Err(ProviderError::InvalidResponse)
    }

    fn create_check_run(
        &self,
        pull_request: GitHubPullRequest<'_>,
        check: &CreateCheckRun,
        deadline: OperationDeadline,
    ) -> Result<CheckRunRecord, ProviderError> {
        let route = format!(
            "/repos/{}/{}/check-runs",
            pull_request.repository_owner, pull_request.repository_name
        );
        self.transport.post(&route, check, deadline)
    }
}

#[derive(Serialize)]
struct PageQuery {
    per_page: u8,
    page: u32,
}

#[derive(Serialize)]
struct CheckRunQuery {
    check_name: String,
    filter: &'static str,
    per_page: u8,
    page: u32,
    app_id: u64,
}

fn path_segment(raw: &str) -> String {
    utf8_percent_encode(raw, NON_ALPHANUMERIC).to_string()
}

fn query_route(route: &str, query: &impl Serialize) -> Result<String, ProviderError> {
    let query =
        serde_urlencoded::to_string(query).map_err(|_defect| ProviderError::InvalidResponse)?;
    Ok(format!("{route}?{query}"))
}
