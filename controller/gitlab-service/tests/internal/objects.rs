#![cfg(test)]

use std::time::Duration;

use amiss_controller::ProviderError;
use amiss_controller_gitlab::{GitLabObjectRequest, GitLabObjectResolver as _};
use amiss_fixtures::sha1_oid;
use amiss_wire::model::Oid;
use secrecy::SecretString;

use super::GitLabGitObjects;

const REPOSITORY_URL: &str = "https://gitlab.example/acme/widget.git";

#[test]
fn a_request_for_another_project_or_repository_never_fetches()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::TempDir::new()?;
    let objects = GitLabGitObjects::new(
        scratch.path().to_owned(),
        101,
        REPOSITORY_URL.to_owned(),
        "amiss".to_owned(),
        SecretString::from("token"),
        Duration::from_secs(5),
    )
    .ok_or("the resolver is not configurable")?;

    let mut foreign = request(101)?;
    foreign.repository_url = "https://attacker.invalid/acme/widget.git".to_owned();
    assert_eq!(
        objects.resolve(&foreign),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        objects.resolve(&request(202)?),
        Err(ProviderError::InvalidResponse)
    );
    Ok(())
}

#[test]
fn a_project_identity_must_be_positive() -> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::TempDir::new()?;
    assert!(
        GitLabGitObjects::new(
            scratch.path().to_owned(),
            0,
            REPOSITORY_URL.to_owned(),
            "amiss".to_owned(),
            SecretString::from("token"),
            Duration::from_secs(5),
        )
        .is_none()
    );
    Ok(())
}

fn request(project_id: u64) -> Result<GitLabObjectRequest, Box<dyn std::error::Error>> {
    Ok(GitLabObjectRequest {
        project_id,
        repository_url: REPOSITORY_URL.to_owned(),
        gate_commit: oid(&"a".repeat(40))?,
        base_commit: oid(&"b".repeat(40))?,
        timeout: Duration::from_secs(1),
    })
}

fn oid(raw: &str) -> Result<Oid, Box<dyn std::error::Error>> {
    sha1_oid(raw).ok_or_else(|| "fixture commit is not SHA-1".into())
}
