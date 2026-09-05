#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "integration assertions over exact semantic-evidence and report shapes"
)]

use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::{CandidateBlock, RequestDigests, Setup, SnapshotIdentity};
use amiss_scan::request::controls;
use amiss_scan::{Effects, semantic};
use amiss_wire::assessment::Nullable;
use amiss_wire::controls::Profile;
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use amiss_wire::requests::{ControlsRequest, SuppliedSemanticEvidence};
use amiss_wire::semantic::{PayloadSchema, SemanticEvidence, SemanticProducer, SemanticSubject};

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

fn policy(projection: &str, source: &serde_json::Value) -> String {
    serde_json::to_string(&serde_json::json!({
        "schema": "amiss/scanner-policy",
        "document_includes": [],
        "projection_assertions": [{
            "document": "docs.md",
            "name": "public-api",
            "projection": projection,
            "sink": "previous-code",
            "source": source,
        }],
        "protected_inventory": [],
        "finding_dispositions": [],
    }))
    .unwrap()
}

fn snapshot(commit: &str, tree: &str) -> SnapshotIdentity {
    SnapshotIdentity {
        commit_oid: Oid::new(ObjectFormat::Sha1, commit.to_owned()).unwrap(),
        kind: amiss_wire::requests::GitSnapshotKind::GitCommit,
        object_format: ObjectFormat::Sha1,
        tree_oid: Oid::new(ObjectFormat::Sha1, tree.to_owned()).unwrap(),
    }
}

fn semantic_inputs(
    candidate_identity_digest: Digest,
    complete: bool,
    set: &str,
    records: &[(&str, &str)],
) -> semantic::Inputs {
    let context_digest = hb("test/record-set-context", b"rust public api");
    let evidence = SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest,
            source_report_payload_digest: Nullable::Null,
        },
        producer: SemanticProducer {
            kind: ArtifactId::new("record-set".to_owned()).unwrap(),
            identity: ArtifactId::new("test-rust-public-api".to_owned()).unwrap(),
            version: "1".to_owned(),
            context_digest,
            input_digest: hb("test/record-set-input", b"rust public api output"),
        },
        complete,
        observations: vec![amiss_fixtures::record_set(set, records)],
    };
    let bytes = amiss_wire::semantic::envelope(evidence).unwrap();
    let request = ControlsRequest {
        semantic_evidence: vec![SuppliedSemanticEvidence {
            value: serde_json::from_slice(&bytes).unwrap(),
            expected_context_digest: context_digest,
        }],
        ..ControlsRequest::default()
    };
    controls(&request)
        .expect("the typed record set enters the scanner")
        .semantic
}

fn run(
    pair: &amiss_fixtures::CommitPair,
    complete: bool,
    set: &str,
    records: &[(&str, &str)],
) -> serde_json::Value {
    let setup = Setup {
        engine: engine(),
        profile: Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: snapshot(&pair.base, &pair.base_tree),
        candidate: CandidateBlock::Commit(snapshot(&pair.candidate, &pair.candidate_tree)),
        policy: Effects::default(),
        controls_unavailable: None,
        requests: RequestDigests::default(),
    };
    let shell = SetupShell {
        engine: engine(),
        profile: Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: semantic::Input::Bound(semantic_inputs(
            amiss_scan::report::candidate_identity_digest(&setup).unwrap(),
            complete,
            set,
            records,
        )),
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let repo = Repository::open(pair.root(), ObjectFormat::Sha1).unwrap();
    let base = Oid::new(ObjectFormat::Sha1, pair.base.clone()).unwrap();
    let candidate = Oid::new(ObjectFormat::Sha1, pair.candidate.clone()).unwrap();
    let built = commit_pair(&repo, &engine(), None, &shell, &base, &candidate);
    let envelope: serde_json::Value = crate::support::generated_report(&built.wire());
    envelope["payload"].clone()
}

fn observed(payload: &serde_json::Value) -> Option<&str> {
    payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|row| {
            (row["kind"] == "projection-drift")
                .then(|| row["candidate_fact"]["evidence"]["observed"].as_str())
                .flatten()
        })
}

#[test]
fn exact_record_values_preserve_partial_evidence_laws() {
    let policy = policy(
        "code-text-v1",
        &serde_json::json!({
            "kind": "record-value",
            "set": "rust/public-api",
            "key": "amiss::check",
        }),
    );
    let pair = amiss_fixtures::commit_pair(
        &[("README.md", "# Base\n")],
        &[
            (
                "docs.md",
                "```text\npub fn check()\n```\n[amiss:public-api]: <amiss:projection>\n",
            ),
            (".amiss/scanner-policy.json", &policy),
        ],
    )
    .unwrap();

    let present_partial = run(
        &pair,
        false,
        "rust/public-api",
        &[("amiss::check", "pub fn check()")],
    );
    assert_eq!(observed(&present_partial), None, "{present_partial}");

    for (complete, set, records, reason) in [
        (
            true,
            "rust/public-api",
            vec![("amiss::other", "pub fn other()")],
            "source-record-absent",
        ),
        (
            false,
            "rust/public-api",
            vec![("amiss::other", "pub fn other()")],
            "source-record-unproven",
        ),
        (
            true,
            "rust/other",
            vec![("amiss::check", "pub fn check()")],
            "source-record-set-absent",
        ),
        (
            true,
            "rust/public-api",
            vec![("amiss::check", "pub fn check() -> Result")],
            "content-differs",
        ),
    ] {
        let payload = run(&pair, complete, set, &records);
        assert_eq!(observed(&payload), Some(reason), "{payload}");
        let finding = payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "projection-drift")
            .expect("the record defect is reported");
        assert_eq!(
            finding["candidate_fact"]["evidence"]["source"],
            serde_json::json!({
                "kind": "record-value",
                "set": "rust/public-api",
                "key": "amiss::check",
            })
        );
    }
}

#[test]
fn complete_record_sets_project_byte_sorted_values() {
    let policy = policy(
        "sorted-rows-v1",
        &serde_json::json!({"kind": "record-set", "set": "rust/public-api"}),
    );
    let pair = amiss_fixtures::commit_pair(
        &[("README.md", "# Base\n")],
        &[
            (
                "docs.md",
                "```text\nAlpha\nAlpha\nZulu\n```\n[amiss:public-api]: <amiss:projection>\n",
            ),
            (".amiss/scanner-policy.json", &policy),
        ],
    )
    .unwrap();

    let exact = run(
        &pair,
        true,
        "rust/public-api",
        &[("a", "Zulu"), ("b", "Alpha"), ("c", "Alpha")],
    );
    assert_eq!(observed(&exact), None, "{exact}");

    let reversed_sink = amiss_fixtures::commit_pair(
        &[("README.md", "# Base\n")],
        &[
            (
                "docs.md",
                "```text\nZulu\nAlpha\nAlpha\n```\n[amiss:public-api]: <amiss:projection>\n",
            ),
            (".amiss/scanner-policy.json", &policy),
        ],
    )
    .unwrap();
    let reordered = run(
        &reversed_sink,
        true,
        "rust/public-api",
        &[("a", "Zulu"), ("b", "Alpha"), ("c", "Alpha")],
    );
    assert_eq!(observed(&reordered), Some("content-differs"));
    let evidence = &reordered["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "projection-drift")
        .expect("the order-only mismatch is reported")["candidate_fact"]["evidence"];
    assert_eq!(
        evidence["source"],
        serde_json::json!({"kind": "record-set", "set": "rust/public-api"})
    );
    assert_eq!(evidence["difference"]["ordering_only"], true, "{reordered}");

    let partial = run(
        &pair,
        false,
        "rust/public-api",
        &[("a", "Zulu"), ("b", "Alpha"), ("c", "Alpha")],
    );
    assert_eq!(
        observed(&partial),
        Some("source-record-set-incomplete"),
        "{partial}"
    );

    let missing = run(&pair, true, "rust/other", &[("a", "Alpha")]);
    assert_eq!(
        observed(&missing),
        Some("source-record-set-absent"),
        "{missing}"
    );
}

#[test]
fn complete_record_sets_project_their_exact_count() {
    let policy = policy(
        "decimal-count-v1",
        &serde_json::json!({"kind": "record-set", "set": "rust/public-api"}),
    );
    let pair = amiss_fixtures::commit_pair(
        &[("README.md", "# Base\n")],
        &[
            (
                "docs.md",
                "```text\n2\n```\n[amiss:public-api]: <amiss:projection>\n",
            ),
            (".amiss/scanner-policy.json", &policy),
        ],
    )
    .unwrap();

    let exact = run(
        &pair,
        true,
        "rust/public-api",
        &[("a", "same"), ("b", "same")],
    );
    assert_eq!(observed(&exact), None, "{exact}");
    let partial = run(
        &pair,
        false,
        "rust/public-api",
        &[("a", "same"), ("b", "same")],
    );
    assert_eq!(
        observed(&partial),
        Some("source-record-set-incomplete"),
        "{partial}"
    );
}
