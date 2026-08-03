#![cfg(test)]

use std::time::{Duration, Instant};

use amiss_controller::ProviderError;
use amiss_fixtures::sha1_oid;
use amiss_wire::model::Oid;

use super::{GitObjectSource, ResolveWant, active, read_commits};
use secrecy::SecretString;

#[test]
fn local_object_proof_reads_exact_commit_trees_and_parents()
-> Result<(), Box<dyn std::error::Error>> {
    let pair = amiss_fixtures::commit_pair(&[("README.md", "base")], &[("README.md", "next")])?;
    let base = commit(&pair.base)?;
    let candidate = commit(&pair.candidate)?;
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
    let candidate = commit(&pair.candidate)?;
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
    let candidate = commit(&pair.candidate)?;

    assert_eq!(
        read_commits(pair.root(), [want(&candidate)], Instant::now()),
        Err(ProviderError::Unavailable)
    );
    Ok(())
}

fn commit(raw: &str) -> Result<Oid, Box<dyn std::error::Error>> {
    sha1_oid(raw).ok_or_else(|| "fixture commit is not SHA-1".into())
}

fn want(oid: &Oid) -> ResolveWant<'_> {
    ResolveWant {
        oid,
        reference: "refs/amiss/test/object",
    }
}

fn source(scratch: std::path::PathBuf) -> GitObjectSource {
    GitObjectSource::new(
        scratch,
        "objects",
        "https://forge.example/acme/widget.git".to_owned(),
        "amiss".to_owned(),
        SecretString::from("token".to_owned()),
        Duration::from_secs(30),
    )
    .expect("a strict https source")
}

#[test]
fn a_source_reports_the_repository_it_was_built_for() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    assert_eq!(
        source(scratch.path().to_owned()).repository_url(),
        "https://forge.example/acme/widget.git"
    );
}

/// A want names an exact object and a reference, and the fetch needs time.
#[test]
fn a_resolve_needs_exact_wants_and_a_live_budget() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let source = source(scratch.path().to_owned());
    let oid = sha1_oid(&"b".repeat(40)).expect("a sha1 object id");
    let other = Oid::new(amiss_wire::model::ObjectFormat::Sha256, "c".repeat(64))
        .expect("a sha256 object id");

    assert_eq!(
        source.resolve(
            [ResolveWant {
                oid: &oid,
                reference: "refs/heads/main",
            }],
            Duration::ZERO,
        ),
        Err(ProviderError::InvalidResponse),
        "no time to fetch in"
    );
    assert_eq!(
        source.resolve(
            [ResolveWant {
                oid: &other,
                reference: "refs/heads/main",
            }],
            Duration::from_secs(1),
        ),
        Err(ProviderError::InvalidResponse),
        "an object outside the sha1 grammar"
    );
    assert_eq!(
        source.resolve(
            [ResolveWant {
                oid: &oid,
                reference: "",
            }],
            Duration::from_secs(1),
        ),
        Err(ProviderError::InvalidResponse),
        "a want naming no reference"
    );
}

#[test]
fn a_deadline_already_reached_is_not_active() {
    assert_eq!(active(Instant::now() + Duration::from_secs(30)), Ok(()));
    assert_eq!(active(Instant::now()), Err(ProviderError::Unavailable));
}
