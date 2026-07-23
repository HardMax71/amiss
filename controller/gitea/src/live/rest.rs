use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use secrecy::SecretString;
use serde::de::DeserializeOwned;

use crate::GiteaPullRequest;

use super::model::{
    BranchProtectionRecord, BranchRecord, CommitRecord, CreateReview, PullRequestRecord,
    RefreshData, RepositoryRecord, ReviewRecord, UserRecord,
};
use super::{Config, GiteaClientError, GiteaTimeouts};

mod transport;

use self::transport::Transport;

const PAGE_SIZE: usize = 50;
const MAX_REVIEW_PAGES: u32 = 20;

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
            if batch.len() > PAGE_SIZE {
                return Err(ProviderError::InvalidResponse);
            }
            let complete = batch.len() < PAGE_SIZE;
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

pub(super) fn protection_rule_path(branch: &BranchRecord) -> Result<String, ProviderError> {
    (!branch.effective_branch_protection_name.is_empty())
        .then_some(branch.effective_branch_protection_name.as_str())
        .map(path_segment)
        .ok_or(ProviderError::InvalidResponse)
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
