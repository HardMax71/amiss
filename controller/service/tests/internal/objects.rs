#![cfg(test)]

use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_wire::model::{ObjectFormat, Oid};

use super::{ResolveWant, read_commits};

#[test]
fn local_object_proof_reads_exact_commit_trees_and_parents()
-> Result<(), Box<dyn std::error::Error>> {
    let pair = amiss_fixtures::commit_pair(&[("README.md", "base")], &[("README.md", "next")])?;
    let base = oid(&pair.base)?;
    let candidate = oid(&pair.candidate)?;
    let [read_candidate, read_base] = read_commits(
        pair.root(),
        [want(&candidate), want(&base)],
        Instant::now() + Duration::from_secs(5),
    )?;

    assert_eq!(read_candidate.id, pair.candidate);
    assert_eq!(read_base.id, pair.base);
    assert_eq!(read_candidate.parents, [read_base.id.as_str()]);
    assert_eq!(read_candidate.tree, pair.candidate_tree);
    assert_eq!(read_base.tree, pair.base_tree);
    assert!(read_base.parents.is_empty());
    Ok(())
}

#[test]
fn a_read_tree_is_never_the_commit_that_names_it() -> Result<(), Box<dyn std::error::Error>> {
    let pair = amiss_fixtures::commit_pair(&[("README.md", "base")], &[("README.md", "next")])?;
    let candidate = oid(&pair.candidate)?;
    let [read] = read_commits(
        pair.root(),
        [want(&candidate)],
        Instant::now() + Duration::from_secs(5),
    )?;

    assert_ne!(read.tree, read.id);
    Ok(())
}

#[test]
fn an_expired_object_proof_does_not_touch_the_repository() -> Result<(), Box<dyn std::error::Error>>
{
    let pair = amiss_fixtures::commit_pair(&[("README.md", "base")], &[("README.md", "next")])?;
    let candidate = oid(&pair.candidate)?;

    assert_eq!(
        read_commits(pair.root(), [want(&candidate)], Instant::now()),
        Err(ProviderError::Unavailable)
    );
    Ok(())
}

fn want(oid: &Oid) -> ResolveWant<'_> {
    ResolveWant {
        oid,
        reference: "refs/amiss/test/object",
    }
}

fn oid(raw: &str) -> Result<Oid, Box<dyn std::error::Error>> {
    Oid::new(ObjectFormat::Sha1, raw.to_owned()).ok_or_else(|| "fixture commit is not SHA-1".into())
}
