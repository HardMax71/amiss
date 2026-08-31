mod tests;

pub(super) use amiss_controller::OperationDeadline;
use amiss_controller::{ForgeFact, ForgeNegative, ProviderError};
pub(super) use amiss_controller::{
    ForgePresence as Presence, ForgeRefFamily as RefFamily, ForgeVisibility as Visibility,
};
use amiss_wire::model::{BranchRef, Oid, RepositoryIdentity};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use secrecy::SecretString;
use serde::de::DeserializeOwned;

use crate::GiteaPullRequest;

use super::model::{
    BranchProtectionRecord, BranchRecord, CommitRecord, CommitStatusRecord, CreateCommitStatus,
    CreateReview, PullRequestRecord, RefRecord, RefreshData, RepositoryRecord, ReviewRecord,
    UserRecord,
};
use super::{Config, GiteaClientError, GiteaTimeouts};

mod transport;

use self::transport::Transport;

const PAGE_SIZE: usize = 50;
const MAX_PAGES: u32 = 20;
// The paginated siblings trust at most ten hundred-row pages; one
// unpaginated answer claiming more than that is not trusted either.
const REF_CEILING: usize = 1000;

/// The read-only verification surface, apart from refresh and publication
/// on purpose: a verifier holding this can state facts and nothing else.
pub(super) trait GiteaVerification: Send + Sync {
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
        revision: &str,
        deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError>;
}

pub(super) trait GiteaRest: Send + Sync {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError>;

    fn current_user(&self, deadline: OperationDeadline) -> Result<UserRecord, ProviderError>;

    fn relation_head(
        &self,
        repository: &RepositoryIdentity,
        target: &BranchRef,
        deadline: OperationDeadline,
    ) -> Result<CommitRecord, ProviderError>;

    fn refresh_data(
        &self,
        config: &Config,
        pull_request: GiteaPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError>;

    fn create_review(
        &self,
        pull_request: GiteaPullRequest<'_>,
        review: &CreateReview,
        deadline: OperationDeadline,
    ) -> Result<ReviewRecord, ProviderError>;

    fn commit_statuses(
        &self,
        repository: &RepositoryIdentity,
        commit: &Oid,
        deadline: OperationDeadline,
    ) -> Result<Vec<CommitStatusRecord>, ProviderError>;

    fn create_commit_status(
        &self,
        repository: &RepositoryIdentity,
        commit: &Oid,
        status: &CreateCommitStatus,
        deadline: OperationDeadline,
    ) -> Result<CommitStatusRecord, ProviderError>;
}

pub(super) struct HttpRest {
    transport: Transport,
}

impl HttpRest {
    pub(super) fn new(
        provider_instance: &str,
        api_base: &str,
        token: SecretString,
        timeouts: GiteaTimeouts,
    ) -> Result<Self, GiteaClientError> {
        Ok(Self {
            transport: Transport::new(provider_instance, api_base, token, timeouts)?,
        })
    }

    fn get<T: DeserializeOwned>(
        &self,
        route: &str,
        deadline: OperationDeadline,
    ) -> Result<T, ProviderError> {
        self.transport.get(route, deadline)
    }

    fn reviews(
        &self,
        pull_request: GiteaPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<Vec<ReviewRecord>, ProviderError> {
        let prefix = repository_route(pull_request.repository_owner, pull_request.repository_name);
        let mut reviews = Vec::new();
        for page in 1..=MAX_PAGES {
            let batch: Vec<ReviewRecord> = self.get(
                &format!(
                    "{prefix}/pulls/{}/reviews?page={page}&limit={PAGE_SIZE}",
                    pull_request.number
                ),
                deadline,
            )?;
            let complete = page_complete(batch.len())?;
            reviews.extend(batch);
            if complete {
                return Ok(reviews);
            }
        }
        Err(ProviderError::InvalidResponse)
    }
}

impl GiteaRest for HttpRest {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        self.transport.deadline()
    }

    fn current_user(&self, deadline: OperationDeadline) -> Result<UserRecord, ProviderError> {
        self.get("/user", deadline)
    }

    fn relation_head(
        &self,
        repository: &RepositoryIdentity,
        target: &BranchRef,
        deadline: OperationDeadline,
    ) -> Result<CommitRecord, ProviderError> {
        let branch = target
            .as_str()
            .strip_prefix("refs/heads/")
            .filter(|branch| !branch.is_empty())
            .ok_or(ProviderError::InvalidResponse)?;
        self.get(
            &format!(
                "{}/git/commits/{}?stat=false&verification=false&files=false",
                repository_route(repository.owner(), repository.name()),
                path_segment(branch)
            ),
            deadline,
        )
    }

    fn refresh_data(
        &self,
        _config: &Config,
        pull_request: GiteaPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError> {
        let prefix = repository_route(pull_request.repository_owner, pull_request.repository_name);
        let reviewer = self.current_user(deadline)?;
        let repository: RepositoryRecord = self.get(&prefix, deadline)?;
        let authoritative: PullRequestRecord =
            self.get(&format!("{prefix}/pulls/{}", pull_request.number), deadline)?;
        let target_branch: BranchRecord = self.get(
            &format!(
                "{prefix}/branches/{}",
                path_segment(&authoritative.base.branch)
            ),
            deadline,
        )?;
        let protection: BranchProtectionRecord = self.get(
            &format!(
                "{prefix}/branch_protections/{}",
                protection_rule_path(&target_branch)?
            ),
            deadline,
        )?;
        let target: CommitRecord = self.get(
            &format!(
                "{prefix}/git/commits/{}",
                path_segment(&authoritative.base.sha)
            ),
            deadline,
        )?;
        let candidate: CommitRecord = self.get(
            &format!(
                "{prefix}/git/commits/{}",
                path_segment(pull_request.candidate_commit.as_str())
            ),
            deadline,
        )?;
        let current_head = if authoritative.head.sha == candidate.sha {
            candidate.clone()
        } else {
            self.get(
                &format!(
                    "{prefix}/git/commits/{}",
                    path_segment(&authoritative.head.sha)
                ),
                deadline,
            )?
        };
        let reviews = self.reviews(pull_request, deadline)?;
        Ok(RefreshData {
            reviewer,
            repository,
            pull_request: authoritative,
            target_branch,
            protection,
            target,
            candidate,
            current_head,
            reviews,
        })
    }

    fn create_review(
        &self,
        pull_request: GiteaPullRequest<'_>,
        review: &CreateReview,
        deadline: OperationDeadline,
    ) -> Result<ReviewRecord, ProviderError> {
        self.transport.post(
            &format!(
                "{}/pulls/{}/reviews",
                repository_route(pull_request.repository_owner, pull_request.repository_name,),
                pull_request.number
            ),
            review,
            deadline,
        )
    }

    fn commit_statuses(
        &self,
        repository: &RepositoryIdentity,
        commit: &Oid,
        deadline: OperationDeadline,
    ) -> Result<Vec<CommitStatusRecord>, ProviderError> {
        let prefix = repository_route(repository.owner(), repository.name());
        let mut statuses = Vec::new();
        for page in 1..=MAX_PAGES {
            let batch: Vec<CommitStatusRecord> = self.get(
                &format!(
                    "{prefix}/statuses/{}?sort=highestindex&page={page}&limit={PAGE_SIZE}",
                    path_segment(commit.as_str())
                ),
                deadline,
            )?;
            let complete = page_complete(batch.len())?;
            statuses.extend(batch);
            if complete {
                return Ok(statuses);
            }
        }
        Err(ProviderError::InvalidResponse)
    }

    fn create_commit_status(
        &self,
        repository: &RepositoryIdentity,
        commit: &Oid,
        status: &CreateCommitStatus,
        deadline: OperationDeadline,
    ) -> Result<CommitStatusRecord, ProviderError> {
        self.transport.post(
            &format!(
                "{}/statuses/{}",
                repository_route(repository.owner(), repository.name()),
                path_segment(commit.as_str())
            ),
            status,
            deadline,
        )
    }
}

// The verification routes carry another repository's spellings, so every
// borrowed segment is percent-encoded rather than trusted.
impl GiteaVerification for HttpRest {
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
            "/repos/{}/{}/git/refs/{}/{}",
            path_segment(owner),
            path_segment(name),
            family.as_str(),
            path_segment(prefix),
        );
        Ok(ref_listing(
            self.transport
                .get_fact::<Vec<RefRecord>>(&route, deadline)?,
            family,
        ))
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
            "/repos/{}/{}/contents/{}?ref={}",
            path_segment(owner),
            path_segment(name),
            encoded.join("/"),
            path_segment(reference),
        );
        let fact = self
            .transport
            .get_fact::<serde::de::IgnoredAny>(&route, deadline)?;
        Ok(fact.map_or_else(
            |negative| match negative {
                ForgeNegative::Missing => Presence::Absent,
                ForgeNegative::Denied => Presence::Unknown,
            },
            |_content| Presence::Present,
        ))
    }

    fn commit_presence(
        &self,
        owner: &str,
        name: &str,
        revision: &str,
        deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        let route = format!(
            "/repos/{}/{}/commits?sha={}&limit=1&stat=false",
            path_segment(owner),
            path_segment(name),
            path_segment(revision),
        );
        self.transport.get_fact(&route, deadline).map(listed_commit)
    }
}

/// The refs route is one unpaginated answer, so completeness is a judgment:
/// a 404 cannot say whether no ref matches or the repository stopped
/// answering mid-walk, and a body past the ceiling is not proven whole.
/// Either way a truncated candidate set could become a false refutation
/// downstream: no fact. Only a 2xx listing within the ceiling is one, and
/// its empty array is the empty match set.
fn ref_listing(fact: ForgeFact<Vec<RefRecord>>, family: RefFamily) -> Option<Vec<String>> {
    let qualifier = format!("refs/{}/", family.as_str());
    match fact {
        Ok(records) if records.len() <= REF_CEILING => Some(
            records
                .into_iter()
                .filter_map(|record| record.reference.strip_prefix(&qualifier).map(str::to_owned))
                .collect(),
        ),
        Ok(_) | Err(ForgeNegative::Missing | ForgeNegative::Denied) => None,
    }
}

/// The commit list route answers 200 with an empty array for an empty
/// repository, whatever the revision asked: only a listed commit is
/// presence, and the empty page is no fact.
fn listed_commit(fact: ForgeFact<Vec<serde::de::IgnoredAny>>) -> Presence {
    match fact {
        Ok(commits) if commits.is_empty() => Presence::Unknown,
        Ok(_) => Presence::Present,
        Err(ForgeNegative::Missing) => Presence::Absent,
        Err(ForgeNegative::Denied) => Presence::Unknown,
    }
}

pub(super) fn protection_rule_path(branch: &BranchRecord) -> Result<String, ProviderError> {
    (!branch.effective_branch_protection_name.is_empty())
        .then_some(branch.effective_branch_protection_name.as_str())
        .map(path_segment)
        .ok_or(ProviderError::InvalidResponse)
}

fn page_complete(batch: usize) -> Result<bool, ProviderError> {
    if batch > PAGE_SIZE {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(batch < PAGE_SIZE)
}

fn repository_route(owner: &str, name: &str) -> String {
    format!("/repos/{}/{}", path_segment(owner), path_segment(name))
}

fn path_segment(raw: &str) -> String {
    utf8_percent_encode(raw, NON_ALPHANUMERIC).to_string()
}
