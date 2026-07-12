#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::Command;

use amiss_git::{GitLimits, GitResources, ObjectKind, Repository, parse_commit};
use amiss_scan::correlate::{Observation, Side, correlate};
use amiss_scan::observe::occurrence_id;
use amiss_scan::report::{Setup, SnapshotIdentity, construct};
use amiss_scan::resolve::TargetCache;
use amiss_scan::{DocumentStatus, ScanLimits, ScanResources, SnapshotDiscovery, discover, resolve};
use amiss_wire::controls::SourceConstruct;
use amiss_wire::digest::hb;
use amiss_wire::json::parse;
use amiss_wire::model::{ObjectFormat, Oid};
use amiss_wire::report::EngineProvenance;
use tempfile::TempDir;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git output utf-8")
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine/v1", b"test engine"),
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
    let discovery = discover(repo, git_resources, &mut scan_resources, &commit.tree).unwrap();

    let mut cache = TargetCache::default();
    let mut observations: Vec<Observation> = Vec::new();
    let mut documents = std::collections::BTreeMap::new();
    for record in &discovery.documents {
        if let Some(raw) = record.raw_digest {
            documents.insert(record.path.clone(), (record.mode, raw));
        }
        let DocumentStatus::Scanned(scanned) = &record.status else {
            continue;
        };
        for occurrence in &scanned.occurrences {
            let is_image = matches!(
                occurrence.occurrence.construct,
                SourceConstruct::InlineImage
                    | SourceConstruct::FullReferenceImage
                    | SourceConstruct::CollapsedReferenceImage
                    | SourceConstruct::ShortcutReferenceImage
            );
            let (intent, resolution) = resolve(
                repo,
                git_resources,
                &mut scan_resources,
                &mut cache,
                &discovery,
                None,
                &record.path,
                is_image,
                &occurrence.occurrence.semantic_destination,
            )
            .unwrap();
            observations.push(Observation {
                id: occurrence_id(
                    &engine(),
                    record.classification.adapter(),
                    &record.path,
                    occurrence,
                    &intent,
                ),
                document: record.path.clone(),
                span: occurrence.occurrence.span,
                display: occurrence.display,
                block_kind: occurrence.occurrence.block_kind,
                node_path: occurrence.occurrence.node_path.clone(),
                adapter: record.classification.adapter(),
                construct: occurrence.occurrence.construct,
                intent,
                raw_destination_digest: occurrence.raw_destination_digest,
                projection_digest: occurrence.projection_digest,
                resolution,
            });
        }
    }
    let identity = SnapshotIdentity {
        object_format: "sha1",
        commit_oid: commit_hex.to_owned(),
        tree_oid: commit.tree.as_str().to_owned(),
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
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README) and [gone](missing.md) and <https://example.com/x>\n",
    )
    .unwrap();
    fs::write(root.join("notes.mdx"), "hello {1 + 1}\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut git_resources = GitResources::new(GitLimits::CONTRACT);
    let (base_identity, base_discovery, base_side) =
        snapshot(&repo, &mut git_resources, &base_commit);
    let (candidate_identity, candidate_discovery, candidate_side) =
        snapshot(&repo, &mut git_resources, &candidate_commit);
    let comparisons = correlate(&base_side, &candidate_side).unwrap();

    let setup = Setup {
        engine: engine(),
        enforce: false,
        repository: None,
        candidate_ref: None,
        default_branch_ref: None,
        base: base_identity,
        candidate: candidate_identity,
    };
    let built = construct(&setup, &base_discovery, &candidate_discovery, &comparisons);

    assert_eq!(built.status, "pass", "observe profile never fails");
    assert_eq!(built.exit_code, 0);
    assert!(built.wire.ends_with(b"\n"));
    let trimmed = built
        .wire
        .get(..built.wire.len().saturating_sub(1))
        .unwrap();
    let reparsed = parse(trimmed).unwrap();
    let mut round_trip = amiss_wire::json::canonical(&reparsed);
    round_trip.push(b'\n');
    assert_eq!(
        round_trip, built.wire,
        "the wire is canonical and round-trips"
    );

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/scanner-report-v1.schema.json"),
    )
    .unwrap()
    .replace("assure/", "amiss/")
    .replace(".assure/", ".amiss/");
    let schema_json: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema_json).unwrap();
    let envelope_json: serde_json::Value = serde_json::from_slice(&built.wire).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope_json)
        .map(|error| format!("{}: {error}", error.instance_path))
        .collect();
    assert_eq!(defects, Vec::<String>::new(), "schema-clean report");

    let payload = envelope_json.get("payload").unwrap();
    assert_eq!(
        payload["result"]["finding_count"].as_u64().unwrap(),
        u64::try_from(payload["findings"].as_array().unwrap().len()).unwrap()
    );
    assert_eq!(
        payload["summary"]["references"]["missing"].as_u64(),
        Some(1)
    );
    assert_eq!(
        payload["summary"]["references"]["external_out_of_scope"].as_u64(),
        Some(1)
    );
    assert_eq!(payload["summary"]["documents"]["scanned"].as_u64(), Some(3));
    let kinds: Vec<&str> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|finding| finding["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"explicit-target-missing"));
    assert!(kinds.contains(&"external-out-of-scope"));
    assert!(
        kinds.contains(&"unlinked-document"),
        "notes.mdx has no links"
    );
}
