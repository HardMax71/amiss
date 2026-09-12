use amiss_wire::ExitClass;
use sha2::Digest as _;

#[test]
fn exit_codes_are_contract() {
    assert_eq!(ExitClass::Success.code(), 0);
    assert_eq!(ExitClass::BlockingFindings.code(), 1);
    assert_eq!(ExitClass::Failure.code(), 2);
}

#[test]
fn reproduces_the_normative_seed_vectors() {
    let gv001 =
        serde_json::from_slice::<serde_json::Value>(br#"{"claim_id":"docs.expr-precedence"}"#)
            .unwrap();
    assert_eq!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("assure/claim-key")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&gv001).unwrap())
                .finalize()
                .0
        )
        .to_string(),
        "sha256:a283ff8a204bef21e06e1932774f08bfe1dc72546aded00e67a18c15cfa98e8a"
    );

    let gv002 = serde_json::from_slice::<serde_json::Value>(br#"{"members":[]}"#).unwrap();
    assert_eq!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("assure/path-set-projection")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&gv002).unwrap())
                .finalize()
                .0
        )
        .to_string(),
        "sha256:434d3282c0603bde1304e3003f386c21c5ab6320ba1adc3e1e4db94ee14a39e2"
    );

    let gv003 =
        serde_json::from_slice::<serde_json::Value>("{\"z\":\"\u{e9}\",\"a\":1}".as_bytes())
            .unwrap();
    assert_eq!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("assure/test-json")
                .chain_update([0_u8])
                .chain_update(serde_json_canonicalizer::to_vec(&gv003).unwrap())
                .finalize()
                .0
        )
        .to_string(),
        "sha256:1bf2a7df49e484b1539f9eb54bc3719ffd8a3383c594e7008d7d844fed89c4bb"
    );

    assert_eq!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("assure/text-projection")
                .chain_update([0_u8])
                .chain_update(b"a\nb\n")
                .finalize()
                .0
        )
        .to_string(),
        "sha256:9094314bad0be6ebcf36a94c249de35e8c0cded01502f6d1d685ee5b1ee6190e"
    );

    assert_eq!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("assure/raw-bytes")
                .chain_update([0_u8])
                .chain_update(b"")
                .finalize()
                .0
        )
        .to_string(),
        "sha256:c214a4103772cd3a23acd41acd40eef154232d1f02848cdcbb67236da126c67e"
    );
}

#[test]
fn domain_separation_changes_the_digest() {
    assert_ne!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/a")
                .chain_update([0_u8])
                .chain_update(b"x")
                .finalize()
                .0
        ),
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/b")
                .chain_update([0_u8])
                .chain_update(b"x")
                .finalize()
                .0
        )
    );
    assert_ne!(
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/a")
                .chain_update([0_u8])
                .chain_update(b"x")
                .finalize()
                .0
        ),
        amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix("amiss/a")
                .chain_update([0_u8])
                .chain_update(b"y")
                .finalize()
                .0
        )
    );
}

/// The identity grammar after the host opened: a host is any nonempty
/// slash-free claim up to the cap, an owner is one or more slash-joined
/// segments, and the github constructor keeps the strict single-segment
/// form GitHub identities can spell.
#[test]
fn the_open_identity_grammar_admits_claims_and_keeps_structure() {
    use amiss_wire::model::{ForgeDialect, RepositoryIdentity};
    let new = |host: &str, owner: &str, name: &str| {
        RepositoryIdentity::new(host.to_owned(), owner.to_owned(), name.to_owned())
    };
    assert!(new("github.com", "acme", "widget").is_some());
    assert!(new("GitHub.com:8080", "acme", "widget").is_some());
    assert!(new("192.168.0.1", "acme", "widget").is_some());
    assert!(new(&"a".repeat(255), "acme", "widget").is_some());
    assert!(new("", "acme", "widget").is_none());
    assert!(new("git/hub.com", "acme", "widget").is_none());
    assert!(new(&"a".repeat(256), "acme", "widget").is_none());

    assert!(new("gitlab.com", "group/subgroup", "widget").is_some());
    assert!(new("gitlab.com", "group//sub", "widget").is_none());
    assert!(new("gitlab.com", "/group", "widget").is_none());
    assert!(new("gitlab.com", "group/", "widget").is_none());
    assert!(new("gitlab.com", "Group", "widget").is_none());
    assert!(new("gitlab.com", "group/-", "widget").is_none());
    let deep = ["a"; 128].join("/");
    assert_eq!(deep.len(), 255);
    assert!(new("gitlab.com", &deep, "widget").is_some());
    assert!(new("gitlab.com", &format!("{deep}/a"), "widget").is_none());

    let github = RepositoryIdentity::github("acme".to_owned(), "widget".to_owned());
    assert_eq!(
        github.as_ref().map(RepositoryIdentity::host),
        Some("github.com")
    );
    assert!(RepositoryIdentity::github("group/sub".to_owned(), "widget".to_owned()).is_none());

    assert_eq!(
        ForgeDialect::default_for_host("github.com"),
        Some(ForgeDialect::Github)
    );
    assert_eq!(
        ForgeDialect::default_for_host("bitbucket.org"),
        Some(ForgeDialect::BitbucketCloud)
    );
    assert_eq!(ForgeDialect::default_for_host("ghes.corp.example"), None);
    assert_eq!(ForgeDialect::Github.as_ref(), "github");
    assert_eq!(ForgeDialect::BitbucketCloud.as_ref(), "bitbucket-cloud");
    assert_eq!(
        ForgeDialect::BitbucketDataCenter.as_ref(),
        "bitbucket-data-center"
    );
}
