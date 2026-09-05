use std::fs;
use std::path::Path;

use amiss_git::{GitLimits, GitResources, ObjectKind, Repository, parse_commit};
use amiss_scan::correlate::{Observation, Side, correlate};
use amiss_scan::observe::{OBSERVATION_ID_DOMAIN, ObservationIdentity, observation_input};
use amiss_scan::report::{
    Built, CandidateBlock, Setup, SnapshotIdentity, construct, construct_incomplete,
};
use amiss_scan::resolve::{Resolver, TargetCache};
use amiss_scan::{
    Classification, DocumentRecord, DocumentStatus, ScanLimits, ScanResources, SnapshotDiscovery,
    discover,
};
use amiss_wire::controls::GitMode;
use amiss_wire::digest::{hb, hj};
use amiss_wire::json::parse;
use amiss_wire::model::{BranchRef, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::model::{DocumentCounts, FindingCounts, ReferenceCounts, Summary};
use amiss_wire::report::{
    AnalysisErrorCode, EngineProvenance, ErrorDetail, MACHINE_JSON_BYTES, adapter_contract,
};
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn snapshot(
    repo: &Repository,
    git_resources: &mut GitResources,
    commit_hex: &str,
) -> (SnapshotIdentity, SnapshotDiscovery, Side) {
    let commit_oid = Oid::new(ObjectFormat::Sha1, commit_hex.to_owned()).unwrap();
    let commit_object = repo
        .read_expected(git_resources, &commit_oid, ObjectKind::Commit)
        .unwrap();
    let commit = parse_commit(ObjectFormat::Sha1, &commit_object.body).unwrap();
    let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
    let discovery = discover(
        repo,
        git_resources,
        &mut scan_resources,
        &amiss_scan::Includes::default(),
        &commit.tree,
    )
    .unwrap();

    let mut cache = TargetCache::default();
    let mut resolver = Resolver::new(
        repo,
        git_resources,
        &mut scan_resources,
        &mut cache,
        &discovery,
    );
    let mut observations: Vec<Observation> = Vec::new();
    let mut documents = std::collections::BTreeMap::new();
    for record in &discovery.documents {
        if let Some(raw) = record.raw_digest {
            documents.insert(record.path.clone(), (record.mode, raw));
        }
        let DocumentStatus::Scanned(scanned) = &record.status else {
            continue;
        };
        let Some(adapter) = record.adapter else {
            continue;
        };
        let adapter_contract_digest = adapter_contract(&engine(), adapter).1;
        for occurrence in &scanned.occurrences {
            let is_image = occurrence.occurrence.construct.is_image();
            let (intent, resolution) = resolver
                .resolve(
                    None,
                    adapter,
                    &record.path,
                    is_image,
                    &occurrence.occurrence.semantic_destination,
                )
                .unwrap();
            let id = hj(
                OBSERVATION_ID_DOMAIN,
                &observation_input(&ObservationIdentity {
                    adapter,
                    contract_digest: adapter_contract_digest,
                    document: &record.path,
                    construct: occurrence.occurrence.construct,
                    node_path: &occurrence.occurrence.node_path,
                    projection_digest: occurrence.projection_digest,
                    intent: &intent,
                    raw_destination_digest: occurrence.raw_destination_digest,
                }),
            );
            observations.push(Observation {
                id,
                adapter_contract_digest,
                document: record.path.clone(),
                span: occurrence.occurrence.span,
                display: occurrence.display,
                block_kind: occurrence.occurrence.block_kind,
                node_path: occurrence.occurrence.node_path.clone(),
                adapter,
                construct: occurrence.occurrence.construct,
                external_destination: matches!(
                    resolution,
                    amiss_wire::resolution::Resolution::External { .. }
                )
                .then(|| occurrence.occurrence.semantic_destination.clone()),
                intent,
                raw_destination: occurrence.occurrence.raw_destination.clone(),
                raw_destination_digest: occurrence.raw_destination_digest,
                projection_digest: occurrence.projection_digest,
                resolution,
                fragment_span: occurrence.occurrence.fragment_span,
                path_span: occurrence.occurrence.path_span,
            });
        }
    }
    let identity = SnapshotIdentity {
        commit_oid: Oid::new(ObjectFormat::Sha1, commit_hex.to_owned()).unwrap(),
        kind: amiss_wire::requests::GitSnapshotKind::GitCommit,
        object_format: ObjectFormat::Sha1,
        tree_oid: commit.tree,
    };
    (
        identity,
        discovery,
        Side {
            observations,
            documents,
        },
    )
}

fn report_between(root: &Path, base_commit: &str, candidate_commit: &str) -> Built {
    report_retaining(root, base_commit, candidate_commit, 64)
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn report_retaining(
    root: &Path,
    base_commit: &str,
    candidate_commit: &str,
    errors_retained: u64,
) -> Built {
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let (base_identity, base_discovery, base_side) = snapshot(&repo, &mut resources, base_commit);
    let (candidate_identity, candidate_discovery, candidate_side) =
        snapshot(&repo, &mut resources, candidate_commit);
    let comparisons = correlate(base_side, candidate_side).unwrap();
    let setup = Setup {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: base_identity,
        candidate: CandidateBlock::Commit(candidate_identity),
        policy: amiss_scan::Effects {
            errors_retained,
            ..amiss_scan::Effects::default()
        },
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    };
    construct(
        &setup,
        &base_discovery,
        &candidate_discovery,
        comparisons,
        &[],
    )
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test assertion helper"
)]
fn assert_grouped_feedback(payload: &serde_json::Value) {
    let feedback = &payload["feedback"];
    assert_eq!(feedback["status"], "available");
    assert_eq!(feedback["existing_count"], 1);
    let items = feedback["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0]["action"], "fix");
    assert_eq!(items[0]["target"], "docs/new-missing.md");
    assert_eq!(
        items[0]["finding_kinds"],
        serde_json::json!(["explicit-target-missing"])
    );
    assert_eq!(items[0]["location_count"], 2);
    assert_eq!(items[0]["annotation"]["path"], "docs/fix-one.md");
    assert_eq!(items[1]["action"], "fix");
    assert_eq!(items[1]["target"], "src/target.rs");
    assert_eq!(
        items[1]["finding_kinds"],
        serde_json::json!(["explicit-target-missing", "explicit-target-type-mismatch"])
    );
    assert_eq!(items[1]["location_count"], 2);
    assert_eq!(items[2]["action"], "check");
    assert_eq!(items[2]["target"], "docs/target.md");
    assert_eq!(items[2]["location_count"], 2);
    assert!(items[2]["annotation"].is_null());
    assert_eq!(items[3]["action"], "existing");
    assert_eq!(items[3]["target"], "docs/missing.md");
    assert_eq!(
        items[3]["finding_kinds"],
        serde_json::json!(["explicit-target-missing"])
    );
    assert_eq!(items[3]["location_count"], 1);
    assert!(items[3]["annotation"].is_null());
}

#[test]
fn a_complete_report_validates_against_the_schema() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README) and [gone](missing.md)\n",
    )
    .unwrap();
    fs::write(root.join("docs/target.md"), "# Before\n").unwrap();
    fs::write(root.join("docs/watch-one.md"), "[target](target.md)\n").unwrap();
    fs::write(root.join("docs/watch-two.md"), "[target](target.md)\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/target.rs"), "fn target() {}\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(root.join("docs/target.md"), "# After\n").unwrap();
    fs::write(root.join("docs/fix-one.md"), "[missing](new-missing.md)\n").unwrap();
    fs::write(root.join("docs/fix-two.md"), "[missing](new-missing.md)\n").unwrap();
    fs::write(root.join("docs/external.md"), "<https://example.com/x>\n").unwrap();
    fs::write(
        root.join("docs/mixed-line.md"),
        "[line](../src/target.rs#L999)\n",
    )
    .unwrap();
    fs::write(
        root.join("docs/mixed-tree.md"),
        "[tree](../src/target.rs/)\n",
    )
    .unwrap();
    fs::write(root.join("notes.mdx"), "hello {1 + 1}\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let built = report_between(root, &base_commit, &candidate_commit);

    assert_eq!(built.status, "pass", "observe profile never fails");
    assert_eq!(built.exit_code, 0);
    let envelope_json: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();

    let payload = envelope_json.get("payload").unwrap();
    assert_eq!(
        payload["result"]["finding_count"].as_u64().unwrap(),
        u64::try_from(payload["findings"].as_array().unwrap().len()).unwrap()
    );
    assert_eq!(
        payload["summary"]["references"]["missing"].as_u64(),
        Some(4)
    );
    assert_eq!(
        payload["summary"]["references"]["external_out_of_scope"].as_u64(),
        Some(1)
    );
    assert_eq!(
        payload["summary"]["documents"]["scanned"].as_u64(),
        Some(11)
    );
    assert_grouped_feedback(payload);
    let kinds: Vec<&str> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"explicit-target-missing"));
    assert!(!kinds.contains(&"unlinked-document"));
    assert!(
        !kinds.contains(&"external-out-of-scope"),
        "an external URL is an observation, not a finding"
    );
    assert_external_destinations(payload);
}

/// The engine never fetches an external URL, so it raises no finding and keeps
/// the destination where it was seen, for the layer that does fetch. Every
/// external resolution carries one and nothing else does.
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test fixture helper"
)]
fn assert_external_destinations(payload: &serde_json::Value) {
    let mut external = 0_usize;
    for row in payload["observations"].as_array().unwrap() {
        for side in ["base", "candidate"] {
            let Some(entry) = row[side].as_object() else {
                continue;
            };
            if entry["resolution"]["kind"] == "external" {
                external = external.saturating_add(1);
                assert_eq!(
                    entry["external_destination"], "https://example.com/x",
                    "an external observation names the destination the source decoded to"
                );
            } else {
                assert!(
                    !entry.contains_key("external_destination"),
                    "{:?} is not external and names no destination",
                    entry["resolution"]["kind"]
                );
            }
        }
    }
    assert_eq!(external, 1, "the fixture holds one external reference");
}

#[test]
fn invalid_references_split_new_existing_and_ambiguous_feedback() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/existing.md"), "[x](../../escape)\n").unwrap();
    fs::write(
        root.join("docs/ambiguous.md"),
        "base wording [x](../../shared)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(root.join("docs/new.md"), "[x](../../new)\n").unwrap();
    fs::write(
        root.join("docs/ambiguous.md"),
        "first wording [x](../../shared)\n\nsecond wording [x](../../shared)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let built = report_between(root, &base, &candidate);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let feedback = &wire["payload"]["feedback"];
    assert_eq!(feedback["existing_count"], 1);
    let items = feedback["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["action"], "fix");
    assert!(items[0]["target"].is_null());
    assert_eq!(items[0]["location_count"], 1);
    assert_eq!(items[0]["annotation"]["path"], "docs/new.md");
    assert_eq!(items[1]["action"], "check");
    assert!(items[1]["target"].is_null());
    assert_eq!(items[1]["location_count"], 2);
    assert!(items[1]["annotation"].is_null());
    assert_eq!(items[2]["action"], "existing");
    assert!(items[2]["target"].is_null());
    assert_eq!(items[2]["location_count"], 1);
    assert!(items[2]["annotation"].is_null());
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn mixed_findings(root: &Path) -> serde_json::Value {
    git(root, &["init", "-q"]);
    fs::write(root.join("hub.md"), "[b](b.md) and [gone](missing.md)\n").unwrap();
    fs::write(root.join("b.md"), "# B\n\n<div>opaque</div>\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("hub.md"),
        "[b](b.md) and [gone](missing.md) and [fresh](fresh.md)\n",
    )
    .unwrap();
    fs::write(root.join("orphan.md"), "# Orphan\n\n<div>opaque</div>\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let built = report_between(root, &base, &candidate);
    crate::support::generated_report(&built.wire()).unwrap()
}

/// A document-scope finding embeds the row of the document it stands on, not
/// whichever row the search happened to reach first.
#[test]
fn a_document_fact_carries_its_own_document_row() {
    let dir = TempDir::new().unwrap();
    let wire = mixed_findings(dir.path());
    let rows = wire["payload"]["findings"].as_array().unwrap();
    let pairs: Vec<(&str, &str)> = rows
        .iter()
        .filter(|row| row["candidate_fact"]["evidence"]["kind"] == "document")
        .map(|row| {
            (
                row["location"]["path"].as_str().unwrap(),
                row["candidate_fact"]["evidence"]["document_result"]["path"]
                    .as_str()
                    .unwrap(),
            )
        })
        .collect();
    assert!(
        pairs.len() >= 2,
        "the fixture emits more than one document finding: {pairs:?}"
    );
    for (finding_path, fact_path) in pairs {
        assert_eq!(finding_path, fact_path);
    }
}

/// Observation facts resolve from both correlation-order runs: base-only
/// rows and rows with a candidate primary.
#[test]
fn an_observation_fact_carries_its_own_comparison_row() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/removed.md"), "[old](target.md)\n").unwrap();
    fs::write(root.join("docs/changed.md"), "[before](target.md)\n").unwrap();
    fs::write(root.join("docs/target.md"), "# Target\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("docs/removed.md"), "No link.\n").unwrap();
    fs::write(root.join("docs/changed.md"), "[after](target.md)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let wire: serde_json::Value =
        crate::support::generated_report(&report_between(root, &base, &candidate).wire()).unwrap();
    let findings = wire["payload"]["findings"].as_array().unwrap();

    for kind in ["explicit-reference-removed", "subject-changed"] {
        let finding = findings
            .iter()
            .find(|finding| finding["kind"] == kind)
            .unwrap_or_else(|| panic!("missing {kind} finding"));
        let observation_id = &finding["observation_ids"][0];
        let comparison = &finding["candidate_fact"]["evidence"]["comparison"];
        let primary = comparison["candidate"]["observation_id"]
            .as_str()
            .or_else(|| comparison["base"]["observation_id"].as_str());
        assert_eq!(observation_id.as_str(), primary, "{kind} evidence row");
    }
}

/// Every attribution counter counts its own class.
#[test]
fn the_summary_counts_each_attribution_it_names() {
    let dir = TempDir::new().unwrap();
    let wire = mixed_findings(dir.path());
    let payload = &wire["payload"];
    let rows = payload["findings"].as_array().unwrap();
    let counted = |attribution: &str| {
        rows.iter()
            .filter(|row| row["attribution"] == attribution)
            .count()
    };
    let reported = |name: &str| payload["summary"]["findings"][name].as_u64().unwrap();

    let classes = [
        ("introduced", "introduced"),
        ("pre_existing", "pre-existing"),
        ("resolved", "resolved"),
        ("unknown", "unknown"),
        ("not_applicable", "not-applicable"),
    ];
    let present = classes
        .iter()
        .filter(|(_, attribution)| counted(attribution) > 0)
        .count();
    assert!(
        present >= 2,
        "the fixture mixes attributions, or the counts prove nothing: {rows:?}"
    );
    for (name, attribution) in classes {
        assert_eq!(
            reported(name),
            u64::try_from(counted(attribution)).unwrap(),
            "{name}"
        );
    }
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn bare_setup(errors_retained: u64) -> Setup {
    let oid = Oid::new(ObjectFormat::Sha1, "a".repeat(40)).unwrap();
    let identity = SnapshotIdentity {
        commit_oid: oid.clone(),
        kind: amiss_wire::requests::GitSnapshotKind::GitCommit,
        object_format: ObjectFormat::Sha1,
        tree_oid: oid,
    };
    Setup {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: identity.clone(),
        candidate: CandidateBlock::Commit(identity),
        policy: amiss_scan::Effects {
            errors_retained,
            ..amiss_scan::Effects::default()
        },
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    }
}

#[test]
fn an_observation_row_hashes_the_identity_input_it_renders() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "[target](target.md)\n").unwrap();
    fs::write(root.join("target.md"), "# Target\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "snapshot"]);
    let commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let (identity, discovery, base) = snapshot(&repo, &mut resources, &commit);
    let (_, _, candidate) = snapshot(&repo, &mut resources, &commit);
    let mut comparisons = correlate(base, candidate).unwrap();
    let wrong = hb("amiss/test-wrong-observation-id", b"wrong");
    let comparison = comparisons.first_mut().unwrap();
    comparison.base.as_mut().unwrap().id = wrong;
    comparison.candidate.as_mut().unwrap().id = wrong;

    let mut setup = bare_setup(64);
    setup.base = identity.clone();
    setup.candidate = CandidateBlock::Commit(identity);
    let built = construct(&setup, &discovery, &discovery, comparisons, &[]);
    let envelope: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let row = &envelope["payload"]["observations"][0]["candidate"];
    let input_bytes = serde_json::to_vec(&row["observation_id_input"]).unwrap();
    let input = parse(&input_bytes).unwrap();
    let expected = hj(OBSERVATION_ID_DOMAIN, &input).to_string();

    assert_ne!(row["observation_id"], wrong.to_string());
    assert_eq!(row["observation_id"], expected);
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn excluded_discovery(paths: &[&str]) -> SnapshotDiscovery {
    let oid = Oid::new(ObjectFormat::Sha1, "b".repeat(40)).unwrap();
    SnapshotDiscovery {
        labels: std::collections::BTreeMap::new(),
        documents: paths
            .iter()
            .map(|path| DocumentRecord {
                path: RepoPath::new((*path).to_owned()).unwrap(),
                classification: Classification::StructuredMarkdown,
                adapter: Some(amiss_wire::model::Adapter::Markdown),
                status: DocumentStatus::ExcludedBuiltIn,
                oid: oid.clone(),
                mode: GitMode::RegularFile,
                byte_count: 0,
                raw_digest: None,
            })
            .collect(),
        outside_document_set: 0,
        tree_entries: u64::try_from(paths.len()).unwrap_or(u64::MAX),
        path_defects: Vec::new(),
        entries: std::collections::BTreeMap::new(),
    }
}

#[test]
fn document_rows_merge_both_sides_in_strict_raw_path_order() {
    let base = excluded_discovery(&["a-.md", "a/base.md", "a0.md"]);
    let candidate = excluded_discovery(&["a/candidate.md", "a0.md", "a1.md"]);
    let built = construct(&bare_setup(64), &base, &candidate, Vec::new(), &[]);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let rows = wire["payload"]["documents"].as_array().unwrap();
    let actual: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            (
                row["path"].as_str().unwrap().to_owned(),
                row["change"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            ("a-.md".to_owned(), "removed".to_owned()),
            ("a/base.md".to_owned(), "removed".to_owned()),
            ("a/candidate.md".to_owned(), "added".to_owned()),
            ("a0.md".to_owned(), "unchanged".to_owned()),
            ("a1.md".to_owned(), "added".to_owned()),
        ]
    );
}

/// A document standing on both sides is unchanged only when both sides say
/// the same thing, so an entry that moved reads as changed.
#[test]
fn a_document_that_moved_is_not_unchanged() {
    let base = excluded_discovery(&["moved.md", "still.md"]);
    let mut candidate = excluded_discovery(&["moved.md", "still.md"]);
    let moved = candidate.documents.first_mut().unwrap();
    moved.oid = Oid::new(ObjectFormat::Sha1, "c".repeat(40)).unwrap();

    let built = construct(&bare_setup(64), &base, &candidate, Vec::new(), &[]);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let rows = wire["payload"]["documents"].as_array().unwrap();
    let changes: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| {
            (
                row["path"].as_str().unwrap(),
                row["change"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        changes,
        [("moved.md", "changed"), ("still.md", "unchanged")]
    );
}

fn missing_detail(path: &str) -> ErrorDetail {
    ErrorDetail {
        code: AnalysisErrorCode::GitObjectMissing,
        path: RepoPath::new(path.to_owned()),
        path_bytes: None,
        resource: None,
    }
}

#[test]
fn error_overflow_retains_the_lowest_keys_and_the_sentinel() {
    let details: Vec<ErrorDetail> = (0..5)
        .map(|index| missing_detail(&format!("p{index}")))
        .collect();
    let built = construct_incomplete(&bare_setup(3), &details);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let errors = wire["payload"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 3, "E - 1 ordinary errors plus the sentinel");
    assert_eq!(errors[0]["path"], "p0");
    assert_eq!(errors[1]["path"], "p1");
    let sentinel = &errors[2];
    assert_eq!(sentinel["code"], "TOO_MANY_ERRORS");
    assert_eq!(sentinel["phase"], "internal");
    assert_eq!(sentinel["resource"], "typed-analysis-errors-retained");
    assert_eq!(sentinel["configured_limit"], 3);
    assert_eq!(sentinel["observed_lower_bound"], 4);
    assert_eq!(wire["payload"]["result"]["error_count"], 3);
    let summary: Summary = serde_json::from_value(wire["payload"]["summary"].clone()).unwrap();
    assert_eq!(
        summary,
        Summary {
            counts_complete: false,
            documents: DocumentCounts::default(),
            references: ReferenceCounts::default(),
            findings: FindingCounts {
                analysis_errors: 3,
                ..FindingCounts::default()
            },
            governed_claims: 0,
            unattested_claims: 0,
        },
    );
    assert_eq!(
        wire["payload"]["feedback"],
        serde_json::json!({"status": "unavailable"})
    );
    assert_eq!(built.exit_code, 2);
}

#[test]
fn exactly_the_ceiling_emits_the_set_without_the_sentinel() {
    let details: Vec<ErrorDetail> = (0..3)
        .map(|index| missing_detail(&format!("p{index}")))
        .collect();
    let built = construct_incomplete(&bare_setup(3), &details);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let errors = wire["payload"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 3, "at most E keys emit exactly");
    assert!(
        errors.iter().all(|row| row["code"] != "TOO_MANY_ERRORS"),
        "no sentinel below the ceiling"
    );
}

#[test]
fn a_ceiling_of_one_emits_only_the_sentinel() {
    let details = [missing_detail("p0"), missing_detail("p1")];
    let built = construct_incomplete(&bare_setup(1), &details);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let errors = wire["payload"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "E = 1 leaves room only for the sentinel");
    assert_eq!(errors[0]["code"], "TOO_MANY_ERRORS");
    assert_eq!(errors[0]["configured_limit"], 1);
    assert_eq!(errors[0]["observed_lower_bound"], 2);
}

/// A run whose errors reach the retained ceiling still ships its detail; one
/// error past it there is no report to trim.
#[test]
fn the_error_ceiling_is_crossed_above_it_and_not_at_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let claim = "A claim [here][amiss:claim].\n\n[amiss:claim]: ./subject.md \"claim\"\n";
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    for name in ["one", "two", "three"] {
        fs::write(root.join(format!("{name}.md")), claim).unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "governed"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let at_ceiling = report_retaining(root, &base, &candidate, 3);
    let wire: serde_json::Value = crate::support::generated_report(&at_ceiling.wire()).unwrap();
    let payload = &wire["payload"];
    assert_eq!(
        payload["errors"].as_array().unwrap().len(),
        3,
        "the fixture governs three documents: {payload}"
    );
    assert!(
        !payload["documents"].as_array().unwrap().is_empty(),
        "errors at the ceiling keep the detail arrays: {payload}"
    );

    let over_ceiling = report_retaining(root, &base, &candidate, 1);
    let wire: serde_json::Value = crate::support::generated_report(&over_ceiling.wire()).unwrap();
    let payload = &wire["payload"];
    assert!(
        payload["documents"].as_array().unwrap().is_empty(),
        "past the ceiling there is no report to detail: {payload}"
    );
    assert_eq!(payload["errors"][0]["code"], "TOO_MANY_ERRORS");
}

#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test fixture helper"
)]
fn schema_max_items(array: &str) -> u64 {
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&text).unwrap();
    schema["$defs"]["ReportPayload"]["properties"][array]["maxItems"]
        .as_u64()
        .expect("the schema caps its union arrays")
}

/// The schema caps `findings` at 100,000 and a charged ceiling counts them at
/// runtime, tested where the floor tightens it. Which of the two fires first on
/// a findings flood is arithmetic between the cap and the weight of a finding,
/// and the reservation moving to 256 MiB reversed it: the leanest finding this
/// engine constructs is now lighter than the break-even, so a hundred thousand
/// of them fit beneath the wire and the counter is what stops the run. That is
/// the order worth having, since the counter names the flood while the wire cap
/// names a byte total, and the wire cap still backstops findings heavier than
/// the break-even.
///
/// The margin is stated nowhere else, so it is stated here against the leanest
/// finding the engine builds today. Fatten the finding shape past the break-even
/// and this fails, which is the signal that the counter has gone back to being
/// unreachable and this note is out of date.
#[test]
fn the_findings_counter_fires_before_the_wire_cap() {
    let ceiling = schema_max_items("findings");
    let break_even = MACHINE_JSON_BYTES / (ceiling + 1);

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[gone](nowhere.md)\n",
    )
    .unwrap();
    fs::write(root.join("docs/leaving.md"), "# Leaving\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    fs::remove_file(root.join("docs/leaving.md")).unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "candidate"]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let base = Oid::new(
        ObjectFormat::Sha1,
        git(root, &["rev-parse", "HEAD~1"]).trim().to_owned(),
    )
    .unwrap();
    let candidate = Oid::new(
        ObjectFormat::Sha1,
        git(root, &["rev-parse", "HEAD"]).trim().to_owned(),
    )
    .unwrap();
    let shell = amiss_scan::pipeline::SetupShell {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
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
        semantic: amiss_scan::semantic::Input::None,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let built =
        amiss_scan::pipeline::commit_pair(&repo, &engine(), None, &shell, &base, &candidate);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let findings = wire["payload"]["findings"].as_array().unwrap();

    let kinds: Vec<&str> = findings
        .iter()
        .filter_map(|finding| finding["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"document-removed") && kinds.contains(&"explicit-target-missing"),
        "the fixture carries more than one shape of finding: {kinds:?}"
    );

    let leanest = findings
        .iter()
        .map(|finding| serde_json::to_string(finding).unwrap().len())
        .min()
        .expect("the fixture produces findings");
    assert!(
        u64::try_from(leanest).unwrap() < break_even,
        "a finding is {leanest} bytes and the break-even is {break_even}: \
         {ceiling} of them no longer fit under the wire cap, so the wire cap has \
         gone back to firing first and this margin note is out of date"
    );
}

#[test]
fn an_over_cap_envelope_projects_to_output_limit_exceeded() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/guide.md"), "# Guide\n\n[home](../README)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README) again\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let (base_identity, base_discovery, base_side) =
        snapshot(&repo, &mut git_resources, &base_commit);
    let (candidate_identity, candidate_discovery, candidate_side) =
        snapshot(&repo, &mut git_resources, &candidate_commit);
    let comparisons = correlate(base_side, candidate_side).unwrap();
    let template = comparisons.first().unwrap().clone();

    let filler = template
        .candidate
        .as_ref()
        .or(template.base.as_ref())
        .unwrap()
        .clone();
    let inflated: Vec<_> = (0..300_u32)
        .map(|index| {
            let mut row = template.clone();
            row.alternatives_candidate = (0..64_u32)
                .map(|slot| {
                    let mut alternative = filler.clone();
                    alternative.document =
                        RepoPath::new(format!("{index:03}{slot:02}{}", "a".repeat(4_000))).unwrap();
                    alternative
                })
                .collect();
            row
        })
        .collect();

    let setup = Setup {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: base_identity,
        candidate: CandidateBlock::Commit(candidate_identity),
        policy: amiss_scan::Effects::default(),
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    };
    let built = construct(&setup, &base_discovery, &candidate_discovery, inflated, &[]);

    assert_eq!(built.status, "incomplete");
    assert_eq!(built.exit_code, 2);
    let wire = built.wire();
    assert!(
        u64::try_from(wire.len()).unwrap_or(u64::MAX) < MACHINE_JSON_BYTES,
        "the projection itself fits the reservation"
    );
    let parsed = crate::support::generated_report(&wire).unwrap();
    let errors = parsed["payload"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    let row = &errors[0];
    assert_eq!(row["code"], "OUTPUT_LIMIT_EXCEEDED");
    assert_eq!(row["phase"], "output");
    assert_eq!(row["resource"], "machine-json-bytes");
    assert_eq!(row["configured_limit"], MACHINE_JSON_BYTES);
    assert!(row["observed_lower_bound"].as_u64().unwrap() > MACHINE_JSON_BYTES);
    assert_eq!(parsed["payload"]["findings"].as_array().unwrap().len(), 0);
    assert_eq!(
        parsed["payload"]["observations"].as_array().unwrap().len(),
        0,
        "the fatal projection discards the detail arrays"
    );
}

/// The location span's line and column fields are the observation's real
/// display positions, not placeholders: a reader of the report can open the
/// file at the row the finding names.
#[test]
fn a_finding_location_carries_the_real_display_positions() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README) and [gone](missing.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    git(root, &["commit", "-qm", "candidate", "--allow-empty"]);
    let candidate_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let (base_identity, base_discovery, base_side) =
        snapshot(&repo, &mut git_resources, &base_commit);
    let (candidate_identity, candidate_discovery, candidate_side) =
        snapshot(&repo, &mut git_resources, &candidate_commit);
    let comparisons = correlate(base_side, candidate_side).unwrap();
    let setup = Setup {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        base: base_identity,
        candidate: CandidateBlock::Commit(candidate_identity),
        policy: amiss_scan::Effects::default(),
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    };
    let built = construct(
        &setup,
        &base_discovery,
        &candidate_discovery,
        comparisons,
        &[],
    );
    let envelope: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let span = envelope["payload"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["kind"] == "explicit-target-missing")
        .expect("the missing target is a finding")["location"]["span"]
        .clone();
    assert_eq!(span["start_line"], 3);
    assert_eq!(
        span["start_column"], 23,
        "one-based scalars, [gone] after the prose"
    );
    assert_eq!(span["end_line"], 3);
    assert_eq!(
        span["end_column"], 41,
        "end exclusive, past the closing parenthesis"
    );
}

/// The evaluation echoes the declared identity's host instead of a literal:
/// a run claiming a self-hosted forge says so in its own report.
#[test]
fn the_evaluation_echoes_a_self_hosted_forge_host() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "one\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("README.md"), "two\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let shell = amiss_scan::pipeline::SetupShell {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
        repository: amiss_wire::model::RepositoryIdentity::new(
            "ghes.example".to_owned(),
            "acme".to_owned(),
            "widget".to_owned(),
        ),
        forge: Some(amiss_wire::model::ForgeDialect::Github),
        candidate_ref: BranchRef::new("refs/heads/main".to_owned()),
        target_ref: None,
        default_branch_ref: BranchRef::new("refs/heads/main".to_owned()),
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: amiss_scan::semantic::Input::None,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let base = Oid::new(ObjectFormat::Sha1, base_commit).unwrap();
    let candidate = Oid::new(ObjectFormat::Sha1, candidate_commit).unwrap();
    let built =
        amiss_scan::pipeline::commit_pair(&repo, &engine(), None, &shell, &base, &candidate);
    let wire: serde_json::Value = crate::support::generated_report(&built.wire()).unwrap();
    let repository = &wire["payload"]["evaluation"]["repository"];
    assert_eq!(repository["host"], "ghes.example");
    assert_eq!(repository["owner"], "acme");
    assert_eq!(repository["name"], "widget");
}
