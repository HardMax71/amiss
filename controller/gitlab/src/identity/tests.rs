#![cfg(test)]

use super::{
    canonical_host, canonical_project_path, canonical_repository, parse_delivery_id, repository_url,
};
use amiss_wire::model::RepositoryIdentity;

#[test]
fn a_delivery_id_is_exactly_five_fields_with_a_digest() {
    assert_eq!(parse_delivery_id("oidc/runner/5/jti/abc123"), Some(5));
    assert_eq!(parse_delivery_id("oidc/runner/5/jti/"), None);
    assert_eq!(parse_delivery_id("oidc/runner/5/jti/abc/extra"), None);
    assert_eq!(parse_delivery_id("oidc/runner/0/jti/abc"), None);
}

#[test]
fn a_project_path_needs_both_halves() {
    assert_eq!(
        canonical_project_path("Acme/Widget").as_deref(),
        Some("acme/widget")
    );
    assert_eq!(canonical_project_path("/widget"), None);
    assert_eq!(canonical_project_path("acme/"), None);
}

#[test]
fn a_repository_url_carries_nothing_but_the_path() {
    assert_eq!(
        repository_url("gitlab.example", "acme/widget").as_deref(),
        Some("https://gitlab.example/acme/widget.git")
    );
    assert_eq!(repository_url("gitlab.example", "acme/widget?q"), None);
    assert_eq!(repository_url("gitlab.example", "acme/widget#f"), None);
}

#[test]
fn a_canonical_repository_is_canonical_in_every_part() {
    let canonical = RepositoryIdentity::new(
        "gitlab.example".to_owned(),
        "acme".to_owned(),
        "widget".to_owned(),
    )
    .expect("a plain identity constructs");
    assert!(canonical_repository(&canonical));

    let loud_host = RepositoryIdentity::new(
        "GitLab.example".to_owned(),
        "acme".to_owned(),
        "widget".to_owned(),
    );
    if let Some(identity) = loud_host {
        assert!(!canonical_repository(&identity));
    }
    let loud_path = RepositoryIdentity::new(
        "gitlab.example".to_owned(),
        "Acme".to_owned(),
        "widget".to_owned(),
    );
    if let Some(identity) = loud_path {
        assert!(!canonical_repository(&identity));
    }
}

#[test]
fn a_canonical_host_is_lowercase_dns_with_bounded_labels() {
    for valid in ["gitlab.com", "my-host.example", "a1.io", "x"] {
        assert!(canonical_host(valid), "{valid}");
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
        assert!(!canonical_host(invalid), "{invalid}");
    }
}
