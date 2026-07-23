use std::sync::atomic::AtomicBool;

use amiss_controller_git::{
    ExactFetch, ExactWant, GitFetchBounds, REPOSITORY_TARGET_REF, fetch_exact,
};
use amiss_wire::model::{ObjectFormat, Oid};

#[test]
fn cancellation_prevents_destination_initialization() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("repository");
    std::fs::create_dir(&destination)?;
    let oid = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).ok_or("invalid fixed SHA-1")?;
    let cancelled = AtomicBool::new(true);

    let result = fetch_exact(ExactFetch {
        url: "https://git.example/acme/widget.git",
        wants: &[ExactWant {
            oid: &oid,
            reference: REPOSITORY_TARGET_REF,
        }],
        destination: &destination,
        credential: None,
        bounds: GitFetchBounds::default(),
        cancelled: &cancelled,
    });

    assert!(result.is_err());
    assert!(destination.read_dir()?.next().is_none());
    Ok(())
}

#[test]
fn invalid_urls_fail_without_echoing_the_input() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("repository");
    std::fs::create_dir(&destination)?;
    let oid = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).ok_or("invalid fixed SHA-1")?;
    let cancelled = AtomicBool::new(false);
    let secret_url = "https://token-secret@git.example/acme/widget.git";

    let error = fetch_exact(ExactFetch {
        url: secret_url,
        wants: &[ExactWant {
            oid: &oid,
            reference: REPOSITORY_TARGET_REF,
        }],
        destination: &destination,
        credential: None,
        bounds: GitFetchBounds::default(),
        cancelled: &cancelled,
    })
    .err()
    .ok_or("the embedded credential was accepted")?;

    assert!(!error.to_string().contains("token-secret"));
    assert!(!format!("{error:?}").contains("token-secret"));
    assert!(destination.read_dir()?.next().is_none());
    Ok(())
}
