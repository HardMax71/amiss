#![cfg(test)]

use amiss_wire::model::{ObjectFormat, Oid};

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use secrecy::SecretString;

use super::{
    Action, Arguments, ExactWant, ExactWants, Negotiate, Protocol, active, create_refs,
    credential_username, exact_https_url, exact_wants, http, initialize, private_ref, v2_handshake,
};
use crate::{DEFAULT_GIT_FETCH_LIMITS, ExactFetch, GitCredential, GitFetchBounds, GitFetchError};

#[test]
fn exact_wants_are_sent_without_haves() -> Result<(), Box<dyn std::error::Error>> {
    let oid = gix::ObjectId::from_hex(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")?;
    let mut negotiate = ExactWants { wants: vec![oid] };
    let known = match negotiate.mark_complete_and_common_ref()? {
        Action::MustNegotiate {
            remote_ref_target_known,
        } => remote_ref_target_known,
        Action::NoChange | Action::SkipToRefUpdate => Vec::new(),
    };
    assert_eq!(known, [false]);

    let mut arguments = Arguments::new(Protocol::V2, Vec::new(), false);
    assert!(negotiate.add_wants(&mut arguments, &known));
    let projected = format!("{arguments:?}");
    assert!(projected.contains("want aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    assert!(projected.contains("haves: []"));
    Ok(())
}

#[test]
fn accepts_only_strict_credential_free_https_urls() {
    assert!(exact_https_url("https://git.example/acme/widget.git").is_ok());
    for invalid in [
        "http://git.example/acme/widget.git",
        "HTTPS://git.example/acme/widget.git",
        "https://user@git.example/acme/widget.git",
        "https://git.example:443/acme/widget.git",
        "https://git.example/acme/widget.git?token=secret",
        "https://git.example/acme/widget.git#fragment",
        "https://git.example",
    ] {
        assert!(exact_https_url(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn accepts_provider_username_without_embedding_a_credential_in_the_url() {
    for valid in ["x-access-token", "oauth2", "gitea-user", "名前"] {
        assert!(credential_username(valid), "{valid}");
    }
    for invalid in ["", "user:password", "user\nheader"] {
        assert!(!credential_username(invalid), "{invalid}");
    }
}

#[test]
fn accepts_only_sha1_objects_under_private_refs() -> Result<(), Box<dyn std::error::Error>> {
    let sha1 = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).ok_or("invalid fixed SHA-1")?;
    let sha256 = Oid::new(ObjectFormat::Sha256, "b".repeat(64)).ok_or("invalid fixed SHA-256")?;
    assert!(
        exact_wants(&[ExactWant {
            oid: &sha1,
            reference: "refs/amiss/repository/target",
        }])
        .is_ok()
    );
    assert!(
        exact_wants(&[ExactWant {
            oid: &sha256,
            reference: "refs/amiss/repository/target",
        }])
        .is_err()
    );
    assert!(
        exact_wants(&[ExactWant {
            oid: &sha1,
            reference: "refs/heads/main",
        }])
        .is_err()
    );
    Ok(())
}

fn fetch_shell(cancelled: &AtomicBool, bounds: GitFetchBounds) -> ExactFetch<'_> {
    ExactFetch {
        url: "https://git.example/acme/widget.git",
        wants: &[],
        destination: std::path::Path::new("unused"),
        credential: None,
        bounds,
        limits: DEFAULT_GIT_FETCH_LIMITS,
        cancelled,
    }
}

#[test]
fn a_fetch_is_active_until_cancelled_or_out_of_time() {
    let calm = AtomicBool::new(false);
    let generous = GitFetchBounds {
        request: Duration::from_mins(10),
    };
    assert_eq!(
        active(&fetch_shell(&calm, generous), Instant::now()),
        Ok(())
    );

    let cancelled = AtomicBool::new(true);
    assert!(active(&fetch_shell(&cancelled, generous), Instant::now()).is_err());

    let spent = GitFetchBounds {
        request: Duration::ZERO,
    };
    assert!(active(&fetch_shell(&calm, spent), Instant::now()).is_err());
}

#[test]
fn initialization_requires_an_empty_destination() {
    let occupied = tempfile::TempDir::new().expect("tempdir");
    std::fs::write(occupied.path().join("squatter"), b"here first").expect("write");
    assert!(initialize(occupied.path()).is_err());

    let empty = tempfile::TempDir::new().expect("tempdir");
    let repository = initialize(empty.path()).expect("an empty destination initializes");
    assert_eq!(repository.object_hash(), gix::hash::Kind::Sha1);
}

#[test]
fn a_private_ref_is_one_clean_amiss_path() {
    assert!(private_ref("refs/amiss/heads/x-1_2"));
    for foreign in [
        "refs/amiss/",
        "refs/amiss//x",
        "refs/amiss/x/",
        "refs/amiss/x//y",
        "refs/amiss/x y",
        "refs/heads/main",
    ] {
        assert!(!private_ref(foreign), "{foreign}");
    }
}

#[test]
fn a_handshake_refuses_a_bad_credential_before_any_transport() {
    let parsed = exact_https_url("https://handshake.invalid/acme/widget.git").expect("url");
    let mut transport = http::Transport::new_http(
        http::reqwest::Remote::default(),
        parsed,
        Protocol::V2,
        false,
    );
    let password = SecretString::from("secret");
    let credential = GitCredential {
        username: "user:name",
        password: &password,
    };
    assert_eq!(
        v2_handshake(&mut transport, Some(credential)).err(),
        Some(GitFetchError("the exact Git credential is invalid"))
    );

    let unreachable = v2_handshake(&mut transport, None);
    assert!(
        unreachable.is_err(),
        "an unresolvable host cannot handshake"
    );
}

#[test]
fn refs_are_created_only_over_objects_the_pack_delivered() {
    let empty = tempfile::TempDir::new().expect("tempdir");
    let repository = initialize(empty.path()).expect("initialize");
    let oid = gix::ObjectId::from_hex(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("oid");
    assert_eq!(
        create_refs(&repository, &[(oid, "refs/amiss/heads/x".to_owned())]),
        Err(GitFetchError("the server omitted an exact wanted object"))
    );
}
