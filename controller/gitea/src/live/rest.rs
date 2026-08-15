mod tests;

use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use secrecy::SecretString;
use serde::de::DeserializeOwned;

use crate::GiteaPullRequest;

use super::model::{
    BranchProtectionRecord, BranchRecord, CommitRecord, CreateReview, PullRequestRecord, RefRecord,
    RefreshData, RepositoryRecord, ReviewRecord, UserRecord,
};
use super::{Config, GiteaClientError, GiteaTimeouts};

mod transport;

use self::transport::{Fact, Transport};

const PAGE_SIZE: usize = 50;
const MAX_REVIEW_PAGES: u32 = 20;
// The paginated siblings trust at most ten hundred-row pages; one
// unpaginated answer claiming more than that is not trusted either.
const REF_CEILING: usize = 1000;

#[derive(Clone, Copy)]
pub(super) struct OperationDeadline(Instant);

impl OperationDeadline {
    pub(super) fn after(timeout: Duration) -> Result<Self, ProviderError> {
        Instant::now()
            .checked_add(timeout)
            .map(Self)
            .ok_or(ProviderError::Unavailable)
    }

    pub(super) fn remaining(self) -> Result<Duration, ProviderError> {
        let remaining = self.0.saturating_duration_since(Instant::now());
        (!remaining.is_zero())
            .then_some(remaining)
            .ok_or(ProviderError::Unavailable)
    }
}

/// What the API said about a foreign repository itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Visibility {
    Readable,
    Missing,
    Denied,
}

/// Whether a route's subject exists; Unknown when the API refused the one
/// route without refusing the repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Presence {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RefFamily {
    Heads,
    Tags,
}

impl RefFamily {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Heads => "heads",
            Self::Tags => "tags",
        }
    }
}

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
        let prefix = repository_route(pull_request);
        let mut reviews = Vec::new();
        for page in 1..=MAX_REVIEW_PAGES {
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
                Fact::Found(_) => Presence::Present,
                Fact::Missing => Presence::Absent,
                Fact::Denied => Presence::Unknown,
            },
        )
    }
}

impl GiteaRest for HttpRest {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        self.transport.deadline()
    }

    fn refresh_data(
        &self,
        _config: &Config,
        pull_request: GiteaPullRequest<'_>,
        deadline: OperationDeadline,
    ) -> Result<RefreshData, ProviderError> {
        let prefix = repository_route(pull_request);
        let reviewer: UserRecord = self.get("/user", deadline)?;
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
                repository_route(pull_request),
                pull_request.number
            ),
            review,
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
                Fact::Found(_) => Visibility::Readable,
                Fact::Missing => Visibility::Missing,
                Fact::Denied => Visibility::Denied,
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
        self.presence(&route, deadline)
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
        Ok(listed_commit(self.transport.get_fact(&route, deadline)?))
    }
}

/// The refs route is one unpaginated answer, so completeness is a judgment:
/// a 404 cannot say whether no ref matches or the repository stopped
/// answering mid-walk, and a body past the ceiling is not proven whole.
/// Either way a truncated candidate set could become a false refutation
/// downstream: no fact. Only a 2xx listing within the ceiling is one, and
/// its empty array is the empty match set.
fn ref_listing(fact: Fact<Vec<RefRecord>>, family: RefFamily) -> Option<Vec<String>> {
    let qualifier = format!("refs/{}/", family.as_str());
    match fact {
        Fact::Found(records) if records.len() <= REF_CEILING => Some(
            records
                .into_iter()
                .filter_map(|record| record.reference.strip_prefix(&qualifier).map(str::to_owned))
                .collect(),
        ),
        Fact::Found(_) | Fact::Missing | Fact::Denied => None,
    }
}

/// The commit list route answers 200 with an empty array for an empty
/// repository, whatever the revision asked: only a listed commit is
/// presence, and the empty page is no fact.
fn listed_commit(fact: Fact<Vec<serde::de::IgnoredAny>>) -> Presence {
    match fact {
        Fact::Found(commits) if commits.is_empty() => Presence::Unknown,
        Fact::Found(_) => Presence::Present,
        Fact::Missing => Presence::Absent,
        Fact::Denied => Presence::Unknown,
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

fn repository_route(pull_request: GiteaPullRequest<'_>) -> String {
    format!(
        "/repos/{}/{}",
        path_segment(pull_request.repository_owner),
        path_segment(pull_request.repository_name)
    )
}

fn path_segment(raw: &str) -> String {
    utf8_percent_encode(raw, NON_ALPHANUMERIC).to_string()
}
