#![cfg(test)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{GitFetchBounds, GitFetchError, active, http_options, remaining_timeout};

#[test]
fn fetch_deadline_decreases_and_expires() {
    let limit = Duration::from_secs(10);
    assert_eq!(
        remaining_timeout(limit, Duration::from_secs(3)),
        Some(Duration::from_secs(7))
    );
    assert_eq!(remaining_timeout(limit, limit), None);
    assert_eq!(remaining_timeout(limit, Duration::from_secs(11)), None);
}

#[test]
fn a_fetch_is_active_until_it_is_cancelled() {
    let calm = AtomicBool::new(false);
    assert_eq!(active(&calm), Ok(()));
    calm.store(true, Ordering::Release);
    assert_eq!(
        active(&calm),
        Err(GitFetchError("the exact Git fetch was interrupted"))
    );
}

#[test]
fn every_fetch_refusal_names_itself() {
    assert_eq!(
        GitFetchError("the exact Git fetch was interrupted").to_string(),
        "the exact Git fetch was interrupted"
    );
}

/// The transport follows nothing, verifies every certificate, and says
/// nothing about what it carries.
#[test]
fn http_options_are_strict_and_quiet() {
    use gix::protocol::transport::client::blocking_io::http;

    let bounds = GitFetchBounds::new(Duration::from_secs(30)).expect("bounds");
    let options = http_options(bounds, Instant::now());
    assert_eq!(
        options.follow_redirects,
        http::options::FollowRedirects::None
    );
    assert!(options.ssl_verify);
    assert!(!options.verbose);
    assert!(
        options.backend.is_some(),
        "the request deadline is installed"
    );
}
