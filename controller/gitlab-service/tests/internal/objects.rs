#![cfg(test)]

use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_wire::model::{ObjectFormat, Oid};

use super::read_objects;

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

fn oid(raw: &str) -> Result<Oid, Box<dyn std::error::Error>> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).ok_or_else(|| "fixture commit is not SHA-1".into())
}

fn exact_sha1(raw: &str) -> bool {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).is_some()
}
