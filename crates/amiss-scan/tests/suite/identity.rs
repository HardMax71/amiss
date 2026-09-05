#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "integration assertions over repository-owned identity goldens"
)]

use amiss_scan::observe::{
    OBSERVATION_ID_DOMAIN, ObservationIdentity, intent_value, observation_input,
};
use amiss_scan::report::{
    CANDIDATE_IDENTITY_DOMAIN, CandidateBlock, INDEX_PROJECTION_SCHEMA, SNAPSHOT_SCHEMA, Setup,
    SnapshotIdentity, candidate_identity_digest, synthetic_candidate,
};
use amiss_scan::resolve::Intent;
use amiss_wire::controls::{GitMode, SourceConstruct, TargetKind};
use amiss_wire::digest::{Digest, hb, hj, hj_serde};
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::model::{
    Adapter, BranchRef, ForgeDialect, ObjectFormat, Oid, RepoPath, RepositoryIdentity,
};
use amiss_wire::report::model::ObservationIdInput;
use amiss_wire::report::{EngineProvenance, IntentKind, adapter_contract};
use strum::IntoEnumIterator;

use crate::support;

use support::{ReportSchemaFragment, fixture_bytes};

#[test]
fn unavailable_report_blocks_match_the_shared_models() {
    use amiss_wire::report::model::{
        ControlsUnavailableReason, SnapshotUnavailableReason, UnavailableControls,
        UnavailableSnapshot, UnavailableSnapshotKind, UnavailableStatus,
    };

    let snapshot_schema = ReportSchemaFragment::new("UnavailableSnapshot");
    let controls_schema = ReportSchemaFragment::new("UnavailableControls");
    for request in [None, Some(hb("amiss/test-request", b"unavailable"))] {
        for snapshot_reason in SnapshotUnavailableReason::iter() {
            for controls_reason in ControlsUnavailableReason::iter() {
                let mut setup = setup(CandidateBlock::Unavailable(vec![snapshot_reason]));
                setup.controls_unavailable = Some(controls_reason);
                setup.requests.snapshot = request;
                setup.requests.controls = request;
                let built = amiss_scan::report::construct_incomplete(
                    &setup,
                    &[amiss_wire::report::ErrorDetail {
                        code: amiss_wire::report::AnalysisErrorCode::InternalError,
                        path: None,
                        path_bytes: None,
                        resource: None,
                    }],
                );
                let envelope = support::generated_report(&built.wire());
                let candidate = &envelope["payload"]["evaluation"]["candidate"];
                let controls = &envelope["payload"]["controls"];
                snapshot_schema.assert_value(candidate, snapshot_reason.as_ref());
                controls_schema.assert_value(controls, controls_reason.as_ref());
                assert_eq!(
                    candidate,
                    &serde_json::to_value(UnavailableSnapshot {
                        kind: UnavailableSnapshotKind::Unavailable,
                        reasons: vec![snapshot_reason],
                        request_digest: request,
                    })
                    .unwrap(),
                );
                assert_eq!(
                    controls,
                    &serde_json::to_value(UnavailableControls {
                        request_digest: request,
                        reasons: vec![controls_reason],
                        status: UnavailableStatus::Unavailable,
                    })
                    .unwrap(),
                );
            }
        }
    }
}

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
        commit_oid: Oid::new(ObjectFormat::Sha1, commit.to_string().repeat(40)).unwrap(),
        kind: amiss_wire::requests::GitSnapshotKind::GitCommit,
        object_format: ObjectFormat::Sha1,
        tree_oid: Oid::new(ObjectFormat::Sha1, tree.to_string().repeat(40)).unwrap(),
    }
}

fn setup(candidate: CandidateBlock) -> Setup {
    Setup {
        engine: EngineProvenance {
            version: "0.0.0-test".to_owned(),
            digest: hb("amiss/scanner-engine", b"identity fixture"),
        },
        profile: amiss_wire::controls::Profile::Observe,
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

#[test]
fn an_unavailable_snapshot_cannot_mint_a_candidate_identity() {
    use amiss_wire::report::model::SnapshotUnavailableReason;

    for reason in SnapshotUnavailableReason::iter() {
        let setup = setup(CandidateBlock::Unavailable(vec![reason]));
        assert!(matches!(
            candidate_identity_digest(&setup),
            Err(amiss_scan::Error::Internal)
        ));
    }
}

#[test]
fn streamed_observation_digests_match_text_and_byte_path_values() {
    let engine = EngineProvenance {
        version: "quoted \"version\"\nβ".to_owned(),
        digest: hb("amiss/scanner-engine", b"observation differential"),
    };
    let text_path =
        RepoPath::new("docs/quoted-\"β.md".to_owned()).expect("the text fixture path is canonical");
    let byte_path = RepoPath::from_bytes(b"docs/byte-\xff.md".to_vec())
        .expect("the byte fixture path is canonical");
    let boundary_path = RepoPath::from_bytes(
        b"docs/"
            .iter()
            .copied()
            .chain(std::iter::repeat_n(0xff, 4091))
            .collect(),
    )
    .unwrap();
    let intents = [
        Intent {
            kind: IntentKind::ExternalUrl,
            commit_oid: None,
            repository_path: None,
            target_kind: None,
            external_scheme: Some("https".to_owned()),
            query: Some(String::new()),
            fragment: Some("fragment".to_owned()),
        },
        Intent {
            kind: IntentKind::RepositoryPath,
            commit_oid: None,
            repository_path: Some(byte_path.clone()),
            target_kind: Some(TargetKind::Either),
            external_scheme: None,
            query: None,
            fragment: None,
        },
        Intent {
            kind: IntentKind::SameRepositoryGithub,
            commit_oid: Oid::new(ObjectFormat::Sha1, "a".repeat(40)),
            repository_path: Some(text_path.clone()),
            target_kind: Some(TargetKind::Blob),
            external_scheme: None,
            query: None,
            fragment: Some("historical".to_owned()),
        },
    ];
    let node_path = [0, 42, usize::MAX];
    let projection_digest = hb("amiss/source-projection", b"projection");
    let raw_destination_digest = hb("amiss/raw-destination", b"destination");
    let historical_intent: serde_json::Value = serde_json::from_slice(&canonical(&intent_value(
        &intents[2],
        raw_destination_digest,
    )))
    .expect("the historical intent is JSON");
    ReportSchemaFragment::new("TargetIntent")
        .assert_value(&historical_intent, "historical target intent");
    for adapter in Adapter::iter() {
        if adapter.metadata().structural_address.is_none() {
            assert_eq!(adapter, Adapter::PlainAdvisory);
            continue;
        }
        let contract_digest = adapter_contract(&engine, adapter).1;
        for (document, intent) in [
            (&text_path, &intents[0]),
            (&byte_path, &intents[1]),
            (&text_path, &intents[2]),
            (&boundary_path, &intents[1]),
        ] {
            for kind in IntentKind::iter() {
                let mut intent = intent.clone();
                intent.kind = kind;
                let identity = ObservationIdentity {
                    adapter,
                    contract_digest,
                    document,
                    construct: SourceConstruct::InlineLink,
                    node_path: &node_path,
                    projection_digest,
                    intent: &intent,
                    raw_destination_digest,
                };
                let input = observation_input(&identity);
                let typed: ObservationIdInput = serde_json::from_slice(&canonical(&input)).unwrap();
                assert_eq!(
                    hj_serde(OBSERVATION_ID_DOMAIN, |writer| serde_json::to_writer(
                        writer, &typed
                    ))
                    .unwrap(),
                    hj(OBSERVATION_ID_DOMAIN, &input),
                    "{} {document:?} {kind:?}",
                    adapter.as_ref()
                );
            }
        }
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
    setup.candidate_ref = BranchRef::new("refs/heads/amiss-controller".to_owned());
    setup.target_ref = BranchRef::new("refs/heads/main".to_owned());
    setup.default_branch_ref = BranchRef::new("refs/heads/main".to_owned());

    let published = fixture_digest(
        "candidate-identity.json",
        "CandidateIdentityInput",
        CANDIDATE_IDENTITY_DOMAIN,
    );
    let gitlab = candidate_identity_digest(&setup).unwrap();
    assert_eq!(gitlab, published);

    setup.forge = Some(ForgeDialect::Github);
    assert_ne!(
        candidate_identity_digest(&setup).unwrap(),
        gitlab,
        "a trusted-time statement cannot be replayed under another URL dialect"
    );

    setup.forge = None;
    assert_ne!(
        candidate_identity_digest(&setup).unwrap(),
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
    let base_commit = Oid::new(ObjectFormat::Sha1, "1".repeat(40)).unwrap();
    let entries = [
        (
            RepoPath::new("README.md".to_owned()).expect("fixture path is canonical"),
            GitMode::RegularFile,
            Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap(),
            false,
        ),
        (
            RepoPath::new("vendor.bin".to_owned()).expect("fixture path is canonical"),
            GitMode::RegularFile,
            Oid::new(ObjectFormat::Sha1, "b".repeat(40)).unwrap(),
            true,
        ),
    ];
    let candidate = synthetic_candidate(ObjectFormat::Sha1, &base_commit, &entries, 1).unwrap();

    assert_eq!(
        candidate.snapshot.index_projection_digest,
        fixture_digest(
            "index-projection.json",
            "IndexProjectionInput",
            INDEX_PROJECTION_SCHEMA,
        ),
    );
    assert_eq!(
        candidate.snapshot.snapshot_digest,
        fixture_digest(
            "synthetic-snapshot.json",
            "SyntheticSnapshotInput",
            SNAPSHOT_SCHEMA,
        ),
    );
    assert_eq!(candidate.snapshot.entry_count, 2);
    assert_eq!(candidate.skip_worktree_paths, 1);

    let setup = setup(CandidateBlock::Index(candidate));
    assert_eq!(
        candidate_identity_digest(&setup).unwrap(),
        fixture_digest(
            "candidate-identity-index.json",
            "CandidateIdentityInput",
            CANDIDATE_IDENTITY_DOMAIN,
        ),
    );
}
