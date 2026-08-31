mod tests;

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
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

use super::Config;
use super::artifact::{
    EXACT_PAGE_SIZE, WorkflowArtifactPage, WorkflowArtifactQuery, WorkflowRunPage,
    WorkflowRunQuery, finish_workflow_artifact, select_workflow_artifact, select_workflow_run,
    validate_workflow_request,
};
use super::model::{
    BranchRule, CheckRunPage, CheckRunRecord, CommitRecord, CreateCheckRun, GateCommitRecord,
    GitCommitRecord, PullRequestRecord, RefRecord, RefreshData, RepositoryCommitRecord,
    RepositoryRecord,
};
use super::relation::GitHubRelationRest;
use super::{GitHubClientError, GitHubTimeouts};

mod transport;

use self::transport::Transport;

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

/// The read-only verification surface, apart from refresh and publication
/// on purpose: a verifier holding this can state facts and nothing else.
pub(super) trait GitHubVerification: Send + Sync {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError>;

    fn repository_visibility(
        &self,
        owner: &str,
        name: &str,
        deadline: OperationDeadline,
    ) -> Result<Visibility, ProviderError>;

    /// Ref names in the family sharing the prefix, family qualifier
    /// stripped; `None` when the repository stopped answering for them or
    /// the listing could not be proven complete, so no ref fact exists.
    fn matching_refs(
        &self,
        owner: &str,
        name: &str,
        family: RefFamily,
        prefix: &str,
        deadline: OperationDeadline,
    ) -> Result<Option<Vec<String>>, ProviderError>;

    /// The path as the tail's decoded segments, each sent to the API as
    /// exactly one segment, so an escaped slash keeps the URL's grouping.
    fn content_presence(
        &self,
        owner: &str,
        name: &str,
        reference: &str,
        path: &[String],
        deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError>;

    fn commit_presence(
        &self,
        owner: &str,
        name: &str,
        oid: &str,
        deadline: OperationDeadline,
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
        let run_page: WorkflowRunPage = self
            .transport
            .get(&query_route(&run_route, &run_query)?, deadline)?;
        let run = select_workflow_run(config, expectation, candidate, run_page)?;

        let artifact_route = format!("/repos/{owner}/{name}/actions/runs/{}/artifacts", run.id);
        let artifact_query = WorkflowArtifactQuery {
            name: &expectation.artifact_name,
            per_page: EXACT_PAGE_SIZE,
            page: 1,
        };
        let artifact_page: WorkflowArtifactPage = self
            .transport
            .get(&query_route(&artifact_route, &artifact_query)?, deadline)?;
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
            let batch: Vec<BranchRule> = self.transport.get(&route, deadline)?;
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
        oid: &str,
        deadline: OperationDeadline,
    ) -> Result<GitCommitRecord, ProviderError> {
        self.transport.get(
            &format!("/repos/{owner}/{name}/git/commits/{}", path_segment(oid)),
            deadline,
        )
    }

    fn presence(
        &self,
        route: &str,
        deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        Ok(
            match self
                .transport
                .get_fact::<serde::de::IgnoredAny>(route, deadline)?
            {
                Ok(_) => Presence::Present,
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

impl GitHubRelationRest for HttpRest {
    fn relation_head(
        &self,
        repository: &RepositoryIdentity,
        target: &BranchRef,
    ) -> Result<CommitRecord, ProviderError> {
        let branch = target
            .as_str()
            .strip_prefix("refs/heads/")
            .filter(|branch| !branch.is_empty())
            .ok_or(ProviderError::InvalidResponse)?;
        let owner = path_segment(repository.owner());
        let name = path_segment(repository.name());
        let branch = path_segment(branch);
        let record: RepositoryCommitRecord = self.transport.get(
            &format!("/repos/{owner}/{name}/commits/{branch}"),
            self.transport.deadline()?,
        )?;
        Ok(CommitRecord {
            sha: record.sha,
            tree: record.commit.tree.sha,
        })
    }
}

// The verification routes carry another repository's spellings, so every
// borrowed segment is percent-encoded rather than trusted.
impl GitHubVerification for HttpRest {
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
                .get_fact::<serde::de::IgnoredAny>(&route, deadline)?
            {
                Ok(_) => Visibility::Readable,
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
    ) -> Result<Option<Vec<String>>, ProviderError> {
        let route = format!(
            "/repos/{}/{}/git/matching-refs/{}/{}",
            path_segment(owner),
            path_segment(name),
            family.as_str(),
            path_segment(prefix),
        );
        let qualifier = format!("refs/{}/", family.as_str());
        let mut names = Vec::new();
        for page in 1..=MAX_PAGES {
            let paged = query_route(
                &route,
                &PageQuery {
                    per_page: PAGE_SIZE_U8,
                    page,
                },
            )?;
            let records = match self
                .transport
                .get_fact::<Vec<RefRecord>>(&paged, deadline)?
            {
                Ok(records) => records,
                Err(ForgeNegative::Missing | ForgeNegative::Denied) => return Ok(None),
            };
            if records.len() > PAGE_SIZE {
                return Err(ProviderError::InvalidResponse);
            }
            let complete = records.len() < PAGE_SIZE;
            names.extend(
                records.into_iter().filter_map(|record| {
                    record.reference.strip_prefix(&qualifier).map(str::to_owned)
                }),
            );
            if complete {
                return Ok(Some(names));
            }
        }
        // Ten full pages leave the listing unproven complete, and a truncated
        // candidate set could become a false refutation downstream: no fact.
        Ok(None)
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
