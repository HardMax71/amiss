#![cfg(test)]

use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_controller_gitlab::GitLabObjectRequest;
use amiss_wire::model::{ObjectFormat, Oid};

use super::{read_objects, validate_request};

#[test]
fn local_object_proof_reads_exact_commit_trees_and_parents()
-> Result<(), Box<dyn std::error::Error>> {
    let pair = amiss_fixtures::commit_pair(&[("README.md", "base")], &[("README.md", "next")])?;
    let base = oid(&pair.base)?;
    let gate = oid(&pair.candidate)?;
    let objects = read_objects(
        pair.root(),
        &gate,
        &base,
        Instant::now() + Duration::from_secs(5),
    )?;

    assert_eq!(objects.gate.id, pair.candidate);
    assert_eq!(objects.base.id, pair.base);
    assert_eq!(objects.gate.parents, [objects.base.id]);
    assert!(exact_sha1(&objects.gate.tree));
    assert!(exact_sha1(&objects.base.tree));
    assert!(objects.base.parents.is_empty());
    Ok(())
}

#[test]
fn an_expired_object_proof_does_not_touch_the_repository() -> Result<(), Box<dyn std::error::Error>>
{
    let pair = amiss_fixtures::commit_pair(&[("README.md", "base")], &[("README.md", "next")])?;
    let base = oid(&pair.base)?;
    let gate = oid(&pair.candidate)?;

    assert_eq!(
        read_objects(pair.root(), &gate, &base, Instant::now()),
        Err(ProviderError::Unavailable)
    );
    Ok(())
}

#[test]
fn object_request_must_match_the_configured_project_and_repository()
-> Result<(), Box<dyn std::error::Error>> {
    let repository_url = "https://gitlab.example/acme/widget.git";
    let request = GitLabObjectRequest {
        project_id: 101,
        repository_url: repository_url.to_owned(),
        gate_commit: oid(&"a".repeat(40))?,
        base_commit: oid(&"b".repeat(40))?,
        timeout: Duration::from_secs(1),
    };
    assert_eq!(validate_request(&request, 101, repository_url), Ok(()));

    let mut attacker = request.clone();
    attacker.repository_url = "https://attacker.invalid/acme/widget.git".to_owned();
    assert_eq!(
        validate_request(&attacker, 101, repository_url),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        validate_request(&request, 202, repository_url),
        Err(ProviderError::InvalidResponse)
    );
    Ok(())
}

fn oid(raw: &str) -> Result<Oid, Box<dyn std::error::Error>> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).ok_or_else(|| "fixture commit is not SHA-1".into())
}

fn exact_sha1(raw: &str) -> bool {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).is_some()
}
