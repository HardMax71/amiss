mod tests;

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::Method;
use secrecy::{SecretSlice, SecretString};
use serde::Serialize;

pub(super) use amiss_controller::OperationDeadline;
use amiss_controller::{
    AcquiredSemanticTemplate, ForgeNegative, ProviderError, WorkflowArtifactExpectation,
};
pub(super) use amiss_controller::{
    ForgePresence as Presence, ForgeRefFamily as RefFamily, ForgeVisibility as Visibility,
};
use amiss_wire::model::{BranchRef, Oid, RepositoryIdentity};

use crate::GitHubPullRequest;
use crate::artifact::WorkflowArtifactPage;

use super::Config;
use super::artifact::{
    EXACT_PAGE_SIZE, WorkflowArtifactQuery, WorkflowRunPage, WorkflowRunQuery,
    finish_workflow_artifact, select_workflow_artifact, select_workflow_run,
    validate_workflow_request,
};
use super::model::{
    CheckRunPage, CheckRunRecord, CommitRecord, CreateCheckRun, GateCommitRecord, GitCommitRecord,
    PullRequestRecord, RefRecord, RefreshData, RepositoryRecord,
};
use super::relation::GitHubRelationRest;
use super::rules::BranchRule;
use super::{GitHubClientError, GitHubTimeouts};

mod transport;

use self::transport::{Transport, decode_body};

const PAGE_SIZE: usize = 100;
const PAGE_SIZE_U8: u8 = 100;
const MAX_PAGES: u32 = 10;

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
        repository: &RepositoryIdentity,
        head_sha: &Oid,
        app_id: u64,
        name: &str,
        deadline: OperationDeadline,
    ) -> Result<Vec<CheckRunRecord>, ProviderError>;

    fn create_check_run(
        &self,
        repository: &RepositoryIdentity,
        check: &CreateCheckRun,
        deadline: OperationDeadline,
    ) -> Result<CheckRunRecord, ProviderError>;
}

/// The read-only verification surface, apart from refresh and publication
/// on purpose: a verifier holding this can state facts and nothing else.
pub(super) trait GitHubVerification: Send + Sync {
    type Deadline: Copy;

    fn deadline(&self) -> Result<Self::Deadline, ProviderError>;

    fn repository_visibility(
        &self,
        owner: &str,
        name: &str,
        deadline: Self::Deadline,
    ) -> Result<Visibility, ProviderError>;

    /// Complete matching records; `None` means no ref fact was acquired.
    fn matching_refs(
        &self,
        owner: &str,
        name: &str,
        family: RefFamily,
        prefix: &str,
        deadline: Self::Deadline,
    ) -> Result<Option<Vec<RefRecord>>, ProviderError>;

    /// The path as the tail's decoded segments, each sent to the API as
    /// exactly one segment, so an escaped slash keeps the URL's grouping.
    fn content_presence(
        &self,
        owner: &str,
        name: &str,
        reference: &str,
        path: &[String],
        deadline: Self::Deadline,
    ) -> Result<Presence, ProviderError>;

    fn commit_presence(
        &self,
        owner: &str,
        name: &str,
        oid: &str,
        deadline: Self::Deadline,
    ) -> Result<Presence, ProviderError>;
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

    pub(super) fn workflow_artifact(
        &self,
        config: &Config,
        expectation: &WorkflowArtifactExpectation,
        candidate: &Oid,
    ) -> Result<AcquiredSemanticTemplate, ProviderError> {
        validate_workflow_request(config, expectation, candidate)?;
        let deadline = self.transport.deadline()?;
        let owner = path_segment(expectation.repository.owner());
        let name = path_segment(expectation.repository.name());
        let workflow = path_segment(expectation.workflow_identity.as_str());
        let run_route = format!("/repos/{owner}/{name}/actions/workflows/{workflow}/runs");
        let run_query = WorkflowRunQuery {
            event: expectation.event.as_str(),
            head_sha: candidate.as_str(),
            status: "success",
            exclude_pull_requests: true,
            per_page: EXACT_PAGE_SIZE,
            page: 1,
        };
        let request = self
            .transport
            .client
            .get(self.transport.url(&query_route(&run_route, &run_query)?)?);
        let run_page: WorkflowRunPage =
            decode_body(self.transport.execute(request, deadline)?, |bytes| {
                serde_json::from_slice(bytes)
            })?;
        let run = select_workflow_run(config, expectation, candidate, run_page)?;

        let artifact_route = format!("/repos/{owner}/{name}/actions/runs/{}/artifacts", run.id);
        let artifact_query = WorkflowArtifactQuery {
            name: &expectation.artifact_name,
            per_page: EXACT_PAGE_SIZE,
            page: 1,
        };
        let request = self.transport.client.get(
            self.transport
                .url(&query_route(&artifact_route, &artifact_query)?)?,
        );
        let artifact_page: WorkflowArtifactPage =
            decode_body(self.transport.execute(request, deadline)?, |bytes| {
                serde_json::from_slice(bytes)
            })?;
        let artifact = select_workflow_artifact(expectation, &run, artifact_page)?;

        let archive_route = format!(
            "/repos/{owner}/{name}/actions/artifacts/{}/zip",
            artifact.id
        );
        let archive = self.transport.download_artifact(
            &archive_route,
            expectation.archive_byte_limit,
            deadline,
        )?;
        finish_workflow_artifact(expectation, artifact, &archive)
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
            let request = self.transport.client.get(self.transport.url(&route)?);
            let batch: Vec<BranchRule> =
                decode_body(self.transport.execute(request, deadline)?, |bytes| {
                    serde_json::from_slice(bytes)
                })?;
            let complete = page_complete(batch.len())?;
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
        oid: &Oid,
        deadline: OperationDeadline,
    ) -> Result<GitCommitRecord, ProviderError> {
        let url = self
            .transport
            .url(&format!("/repos/{owner}/{name}/git/commits/{oid}"))?;
        decode_body(
            self.transport
                .execute(self.transport.client.get(url), deadline)?,
            |bytes| serde_json::from_slice(bytes),
        )
    }

    fn presence(
        &self,
        route: &str,
        deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        Ok(
            match self.transport.request_fact(Method::HEAD, route, deadline)? {
                Ok(_response) => Presence::Present,
                Err(ForgeNegative::Missing) => Presence::Absent,
                Err(ForgeNegative::Denied) => Presence::Unknown,
            },
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
        let url = self.transport.url(&format!(
            "/repos/{}/{}/pulls/{}",
            pull_request.repository_owner, pull_request.repository_name, pull_request.number
        ))?;
        decode_body(
            self.transport
                .execute(self.transport.client.get(url), deadline)?,
            |bytes| amiss_wire::read_json(bytes, u64::MAX),
        )
    }

    fn refresh_data(
        &self,
        pull_request: GitHubPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError> {
        let owner = pull_request.repository_owner;
        let name = pull_request.repository_name;
        let request = self
            .transport
            .client
            .get(self.transport.url(&format!("/repos/{owner}/{name}"))?);
        let repository: RepositoryRecord =
            decode_body(self.transport.execute(request, deadline)?, |bytes| {
                serde_json::from_slice(bytes)
            })?;
        let authoritative = self.pull_request(pull_request, deadline)?;
        let target = self.git_commit(owner, name, &authoritative.base.sha, deadline)?;
        let candidate = self.git_commit(owner, name, pull_request.candidate_commit, deadline)?;
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
            .as_ref()
            .ok_or(ProviderError::Unavailable)?;
        let gate = self.git_commit(owner, name, gate_sha, deadline)?;
        let rules = self.branch_rules(owner, name, &authoritative.base.branch, deadline)?;
        Ok(RefreshData {
            repository,
            pull_request: authoritative,
            target: CommitRecord {
                sha: target.sha,
                tree: target.tree.sha,
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
        repository: &RepositoryIdentity,
        head_sha: &Oid,
        app_id: u64,
        name: &str,
        deadline: OperationDeadline,
    ) -> Result<Vec<CheckRunRecord>, ProviderError> {
        let owner = path_segment(repository.owner());
        let repository_name = path_segment(repository.name());
        let route = format!(
            "/repos/{owner}/{repository_name}/commits/{}/check-runs",
            head_sha.as_str()
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
            let request = self.transport.client.get(self.transport.url(&route)?);
            let response: CheckRunPage =
                decode_body(self.transport.execute(request, deadline)?, |bytes| {
                    serde_json::from_slice(bytes)
                })?;
            let count =
                u64::try_from(runs.len()).map_err(|_defect| ProviderError::InvalidResponse)?;
            check_page(count, response.check_runs.len(), response.total_count)?;
            runs.extend(response.check_runs);
            let count =
                u64::try_from(runs.len()).map_err(|_defect| ProviderError::InvalidResponse)?;
            if runs_settled(count, response.total_count)? {
                return Ok(runs);
            }
        }
        Err(ProviderError::InvalidResponse)
    }

    fn create_check_run(
        &self,
        repository: &RepositoryIdentity,
        check: &CreateCheckRun,
        deadline: OperationDeadline,
    ) -> Result<CheckRunRecord, ProviderError> {
        let route = format!(
            "/repos/{}/{}/check-runs",
            path_segment(repository.owner()),
            path_segment(repository.name())
        );
        let request = self
            .transport
            .client
            .post(self.transport.url(&route)?)
            .json(check);
        decode_body(self.transport.execute(request, deadline)?, |bytes| {
            serde_json::from_slice(bytes)
        })
    }
}

impl GitHubRelationRest for HttpRest {
    fn relation_head(
        &self,
        repository: &RepositoryIdentity,
        target: &BranchRef,
    ) -> Result<(RefRecord, GitCommitRecord), ProviderError> {
        let branch = target
            .as_str()
            .strip_prefix("refs/heads/")
            .filter(|branch| !branch.is_empty())
            .ok_or(ProviderError::InvalidResponse)?;
        let owner = path_segment(repository.owner());
        let name = path_segment(repository.name());
        let branch = path_segment(branch);
        let deadline = self.transport.deadline()?;
        let request = self.transport.client.get(
            self.transport
                .url(&format!("/repos/{owner}/{name}/git/ref/heads/{branch}"))?,
        );
        let reference: RefRecord =
            decode_body(self.transport.execute(request, deadline)?, |bytes| {
                serde_json::from_slice(bytes)
            })?;
        let commit = self.git_commit(&owner, &name, &reference.object.sha, deadline)?;
        Ok((reference, commit))
    }
}

// The verification routes carry another repository's spellings, so every
// borrowed segment is percent-encoded rather than trusted.
impl GitHubVerification for HttpRest {
    type Deadline = OperationDeadline;

    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        self.transport.deadline()
    }

    fn repository_visibility(
        &self,
        owner: &str,
        name: &str,
        deadline: OperationDeadline,
    ) -> Result<Visibility, ProviderError> {
        let route = format!("/repos/{}/{}", path_segment(owner), path_segment(name));
        Ok(
            match self
                .transport
                .request_fact(Method::HEAD, &route, deadline)?
            {
                Ok(_response) => Visibility::Readable,
                Err(ForgeNegative::Missing) => Visibility::Missing,
                Err(ForgeNegative::Denied) => Visibility::Denied,
            },
        )
    }

    fn matching_refs(
        &self,
        owner: &str,
        name: &str,
        family: RefFamily,
        prefix: &str,
        deadline: OperationDeadline,
    ) -> Result<Option<Vec<RefRecord>>, ProviderError> {
        let route = format!(
            "/repos/{}/{}/git/matching-refs/{}/{}",
            path_segment(owner),
            path_segment(name),
            family.as_ref(),
            path_segment(prefix),
        );
        let Ok(response) = self.transport.request_fact(Method::GET, &route, deadline)? else {
            return Ok(None);
        };
        decode_body(response, |bytes| serde_json::from_slice(bytes)).map(Some)
    }

    fn content_presence(
        &self,
        owner: &str,
        name: &str,
        reference: &str,
        path: &[String],
        deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        let encoded: Vec<String> = path.iter().map(|segment| path_segment(segment)).collect();
        let route = format!(
            "/repos/{}/{}/contents/{}",
            path_segment(owner),
            path_segment(name),
            encoded.join("/"),
        );
        let route = query_route(
            &route,
            &ContentQuery {
                reference: reference.to_owned(),
            },
        )?;
        self.presence(&route, deadline)
    }

    fn commit_presence(
        &self,
        owner: &str,
        name: &str,
        oid: &str,
        deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        let route = format!(
            "/repos/{}/{}/commits/{}",
            path_segment(owner),
            path_segment(name),
            path_segment(oid),
        );
        self.presence(&route, deadline)
    }
}

#[derive(Serialize)]
struct ContentQuery {
    #[serde(rename = "ref")]
    reference: String,
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

fn page_complete(batch: usize) -> Result<bool, ProviderError> {
    if batch > PAGE_SIZE {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(batch < PAGE_SIZE)
}

fn check_page(collected: u64, page_len: usize, total: u64) -> Result<(), ProviderError> {
    let maximum = u64::from(PAGE_SIZE_U8)
        .checked_mul(u64::from(MAX_PAGES))
        .ok_or(ProviderError::InvalidResponse)?;
    if page_len > PAGE_SIZE || total < collected || total > maximum {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(())
}

fn runs_settled(collected: u64, total: u64) -> Result<bool, ProviderError> {
    if collected > total {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(collected == total)
}

fn path_segment(raw: &str) -> String {
    utf8_percent_encode(raw, NON_ALPHANUMERIC).to_string()
}

fn query_route(route: &str, query: &impl Serialize) -> Result<String, ProviderError> {
    let query =
        serde_urlencoded::to_string(query).map_err(|_defect| ProviderError::InvalidResponse)?;
    Ok(format!("{route}?{query}"))
}
