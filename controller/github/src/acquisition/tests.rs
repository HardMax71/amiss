#![cfg(test)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use amiss_controller::AcquisitionTarget;
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

use super::{GitHubAcquireError, active, canonical_github_repository, exact_sha1, github_host};

#[test]
fn every_refusal_names_itself() {
    assert_eq!(
        GitHubAcquireError::InvalidRequest.to_string(),
        "the GitHub acquisition request is inconsistent"
    );
    assert_eq!(
        GitHubAcquireError::Credentials.to_string(),
        "the GitHub installation credential is unavailable"
    );
    assert_eq!(
        GitHubAcquireError::Artifact.to_string(),
        "the planned GitHub workflow artifact could not be acquired"
    );
}

#[test]
fn acquisition_is_active_until_cancelled() {
    let target = |cancelled: bool| AcquisitionTarget {
        repository: Path::new("repository"),
        action: Path::new("action"),
        cancelled: Arc::new(AtomicBool::new(cancelled)),
    };
    assert_eq!(active(&target(false)), Ok(()));
    assert_eq!(active(&target(true)), Err(GitHubAcquireError::Cancelled));
}

#[test]
fn a_canonical_github_repository_is_flat_and_lowercase() {
    let identity = |host: &str, owner: &str| {
        RepositoryIdentity::new(host.to_owned(), owner.to_owned(), "widget".to_owned())
    };
    assert!(identity("github.com", "acme").is_some_and(|repo| canonical_github_repository(&repo)));
    if let Some(nested) = identity("github.com", "group/owner") {
        assert!(!canonical_github_repository(&nested), "a nested owner");
    }
    if let Some(loud) = identity("GitHub.com", "acme") {
        assert!(!canonical_github_repository(&loud), "an uppercase host");
    }
}

#[test]
fn a_github_host_is_lowercase_dns_with_bounded_labels() {
    for valid in ["github.com", "github-enterprise.example", "a1.io", "x"] {
        assert!(github_host(valid), "{valid}");
    }
    let overlong = format!("{}a", "a.".repeat(127));
    let long_label = format!("{}.com", "a".repeat(64));
    for invalid in [
        "UPPER.com",
        overlong.as_str(),
        long_label.as_str(),
        "-ab.com",
        "ab-.com",
        "a..b",
        "",
    ] {
        assert!(!github_host(invalid), "{invalid}");
    }
}

#[test]
fn only_a_sha1_oid_is_exact_sha1() {
    let sha1 = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).expect("sha1");
    assert!(exact_sha1(&sha1));
    let sha256 = Oid::new(ObjectFormat::Sha256, "b".repeat(64)).expect("sha256");
    assert!(!exact_sha1(&sha256));
}
