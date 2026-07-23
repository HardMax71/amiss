#![forbid(unsafe_code)]

mod pack;
mod protocol;

use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use amiss_controller::{Acquisition, AcquisitionTarget, RunRequest};
use amiss_wire::model::Oid;
use secrecy::SecretString;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_mins(1);
const MAX_REQUEST_TIMEOUT: Duration = Duration::from_mins(2);

pub const REPOSITORY_TARGET_REF: &str = "refs/amiss/repository/target";
pub const REPOSITORY_CANDIDATE_REF: &str = "refs/amiss/repository/candidate";
pub const ACTION_COMMIT_REF: &str = "refs/amiss/action/commit";

/// One deadline shared by transport, pack receipt, validation, and indexing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitFetchBounds {
    request: Duration,
}

impl GitFetchBounds {
    pub fn new(request: Duration) -> Option<Self> {
        (!request.is_zero() && request.subsec_nanos() == 0 && request <= MAX_REQUEST_TIMEOUT)
            .then_some(Self { request })
    }
}

impl Default for GitFetchBounds {
    fn default() -> Self {
        Self {
            request: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

#[derive(Clone, Copy)]
pub struct GitCredential<'a> {
    pub username: &'a str,
    pub password: &'a SecretString,
}

#[derive(Clone, Copy)]
pub struct ExactWant<'a> {
    pub oid: &'a Oid,
    pub reference: &'a str,
}

#[derive(Clone, Copy)]
pub struct ExactFetch<'a> {
    pub url: &'a str,
    pub wants: &'a [ExactWant<'a>],
    pub destination: &'a Path,
    pub credential: Option<GitCredential<'a>>,
    pub bounds: GitFetchBounds,
    pub cancelled: &'a AtomicBool,
}

pub struct GitRemote {
    pub url: String,
    pub username: String,
    pub password: SecretString,
}

pub struct GitAcquisitionPlan {
    pub repository: GitRemote,
    pub repository_oids: [Oid; 2],
    pub action: GitRemote,
    pub action_oid: Oid,
}

pub struct GitAcquisition<F> {
    pub bounds: GitFetchBounds,
    pub plan: F,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitFetchError(&'static str);

impl fmt::Display for GitFetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for GitFetchError {}

/// Fetches only the named SHA-1 objects over strict HTTPS and installs fixed local refs.
///
/// # Errors
///
/// The URL, credential, object identifiers, destination, transport, or pack is invalid,
/// the deadline expires, or cancellation is requested.
pub fn fetch_exact(fetch: ExactFetch<'_>) -> Result<(), GitFetchError> {
    protocol::fetch_exact(fetch)
}

impl<F, E> Acquisition for GitAcquisition<F>
where
    F: FnMut(&RunRequest) -> Result<GitAcquisitionPlan, E> + Send,
{
    type Error = GitFetchError;

    fn acquire(
        &mut self,
        request: &RunRequest,
        target: AcquisitionTarget<'_>,
    ) -> Result<(), Self::Error> {
        active(target.cancelled.as_ref())?;
        let plan = (self.plan)(request)
            .map_err(|_defect| GitFetchError("the Git fetch plan is invalid"))?;
        let [base, candidate] = &plan.repository_oids;
        fetch_exact(ExactFetch {
            url: &plan.repository.url,
            wants: &[
                ExactWant {
                    oid: base,
                    reference: REPOSITORY_TARGET_REF,
                },
                ExactWant {
                    oid: candidate,
                    reference: REPOSITORY_CANDIDATE_REF,
                },
            ],
            destination: target.repository,
            credential: Some(GitCredential {
                username: &plan.repository.username,
                password: &plan.repository.password,
            }),
            bounds: self.bounds,
            cancelled: target.cancelled.as_ref(),
        })?;
        active(target.cancelled.as_ref())?;
        fetch_exact(ExactFetch {
            url: &plan.action.url,
            wants: &[ExactWant {
                oid: &plan.action_oid,
                reference: ACTION_COMMIT_REF,
            }],
            destination: target.action,
            credential: Some(GitCredential {
                username: &plan.action.username,
                password: &plan.action.password,
            }),
            bounds: self.bounds,
            cancelled: target.cancelled.as_ref(),
        })?;
        active(target.cancelled.as_ref())
    }
}

fn active(cancelled: &AtomicBool) -> Result<(), GitFetchError> {
    (!cancelled.load(Ordering::Acquire))
        .then_some(())
        .ok_or(GitFetchError("the exact Git fetch was interrupted"))
}

fn http_options(
    bounds: GitFetchBounds,
    started: Instant,
) -> gix::protocol::transport::client::blocking_io::http::Options {
    use gix::protocol::transport::client::blocking_io::http;

    let backend = http::reqwest::Options {
        configure_request: Some(Box::new(move |request| {
            let remaining = remaining_timeout(bounds.request, started.elapsed())
                .ok_or_else(fetch_deadline_elapsed)?;
            *request.timeout_mut() = Some(remaining);
            Ok(())
        })),
    };
    http::Options {
        follow_redirects: http::options::FollowRedirects::None,
        ssl_verify: true,
        verbose: false,
        backend: Some(Arc::new(Mutex::new(backend))),
        ..Default::default()
    }
}

fn remaining_timeout(limit: Duration, elapsed: Duration) -> Option<Duration> {
    limit
        .checked_sub(elapsed)
        .filter(|remaining| !remaining.is_zero())
}

fn fetch_deadline_elapsed() -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::from(std::io::ErrorKind::TimedOut))
}

#[path = "../tests/internal/lib.rs"]
mod tests;
