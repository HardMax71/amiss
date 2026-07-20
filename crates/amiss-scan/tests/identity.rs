#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned identity goldens"
)]

use amiss_scan::report::{
    CANDIDATE_IDENTITY_DOMAIN, CandidateBlock, INDEX_PROJECTION_SCHEMA, SNAPSHOT_SCHEMA, Setup,
    SnapshotIdentity, candidate_identity_digest, synthetic_candidate,
};
use amiss_wire::controls::GitMode;
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json::{Value, parse};
use amiss_wire::model::{ForgeDialect, RepoPath, RepositoryIdentity};
use amiss_wire::report::EngineProvenance;

mod support;

use support::{ReportSchemaFragment, fixture_bytes};

fn fixture_digest(name: &str, definition: &str, domain: &str) -> Digest {
    let bytes = fixture_bytes(name);
    let schema_value: serde_json::Value =
        serde_json::from_slice(&bytes).expect("the identity fixture is JSON");
    ReportSchemaFragment::new(definition).assert_value(&schema_value, name);
    let value: Value = parse(&bytes).expect("the identity fixture is strict JSON");
    hj(domain, &value)
}

fn snapshot(commit: char, tree: char) -> SnapshotIdentity {
    SnapshotIdentity {
        object_format: "sha1",
        commit_oid: commit.to_string().repeat(40),
        tree_oid: tree.to_string().repeat(40),
    }
}

fn setup(candidate: CandidateBlock) -> Setup {
    Setup {
        engine: EngineProvenance {
            version: "0.0.0-test".to_owned(),
            digest: hb("amiss/scanner-engine", b"identity fixture"),
        },
        enforce: false,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: snapshot('1', '2'),
        candidate,
        policy: amiss_scan::Effects::default(),
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    }
}

/// The published commit-pair preimage is built by the same identity rows the
/// trusted-time gate hashes. Removing the selected forge remains a different
/// identity even though every Git snapshot and repository field is unchanged.
#[test]
fn the_commit_candidate_identity_fixture_matches_the_runtime_preimage() {
    let mut setup = setup(CandidateBlock::Commit(snapshot('3', '4')));
    setup.repository = RepositoryIdentity::new(
        "gitlab.example.internal".to_owned(),
        "platform/security".to_owned(),
        "docs".to_owned(),
    );
    setup.forge = Some(ForgeDialect::Gitlab);
    setup.candidate_ref = Some("refs/heads/amiss-controller".to_owned());
    setup.target_ref = Some("refs/heads/main".to_owned());
    setup.default_branch_ref = Some("refs/heads/main".to_owned());

    let published = fixture_digest(
        "candidate-identity.json",
        "CandidateIdentityInput",
        CANDIDATE_IDENTITY_DOMAIN,
    );
    let gitlab = candidate_identity_digest(&setup);
    assert_eq!(gitlab, published);

    setup.forge = Some(ForgeDialect::Github);
    assert_ne!(
        candidate_identity_digest(&setup),
        gitlab,
        "a trusted-time statement cannot be replayed under another URL dialect"
    );

    setup.forge = None;
    assert_ne!(
        candidate_identity_digest(&setup),
        gitlab,
        "a trusted-time statement cannot be replayed without its selected URL dialect"
    );
}

/// The staged identity is one chain: complete sorted index projection,
/// synthetic snapshot over that projection, then candidate identity over the
/// snapshot. All three intermediate examples must reproduce the production
/// builder's digests.
#[test]
fn the_staged_identity_fixtures_reproduce_the_runtime_digest_chain() {
    let base_commit = "1".repeat(40);
    let entries = [
        (
            RepoPath::new("README.md".to_owned()).expect("fixture path is canonical"),
            GitMode::RegularFile,
            "a".repeat(40),
            false,
        ),
        (
            RepoPath::new("vendor.bin".to_owned()).expect("fixture path is canonical"),
            GitMode::RegularFile,
            "b".repeat(40),
            true,
        ),
    ];
    let candidate = synthetic_candidate("sha1", &base_commit, &entries, 1);

    assert_eq!(
        candidate.projection_digest,
        fixture_digest(
            "index-projection.json",
            "IndexProjectionInput",
            INDEX_PROJECTION_SCHEMA,
        ),
    );
    assert_eq!(
        candidate.snapshot_digest,
        fixture_digest(
            "synthetic-snapshot.json",
            "SyntheticSnapshotInput",
            SNAPSHOT_SCHEMA,
        ),
    );
    assert_eq!(candidate.entry_count, 2);
    assert_eq!(candidate.skip_worktree_paths, 1);

    let setup = setup(CandidateBlock::Index(candidate));
    assert_eq!(
        candidate_identity_digest(&setup),
        fixture_digest(
            "candidate-identity-index.json",
            "CandidateIdentityInput",
            CANDIDATE_IDENTITY_DOMAIN,
        ),
    );
}
