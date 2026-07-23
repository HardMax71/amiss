#![cfg(test)]

use amiss_wire::model::{ObjectFormat, Oid};

use super::{
    Action, Arguments, ExactWant, ExactWants, Negotiate, Protocol, credential_username,
    exact_https_url, exact_wants,
};

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
