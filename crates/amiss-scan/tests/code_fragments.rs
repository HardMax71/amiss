#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "integration assertions over a controlled report fixture"
)]

use std::fs;
use std::path::Path;

use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::RequestDigests;
use amiss_scan::resolve::ForgeContext;
use amiss_wire::digest::hb;
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::report::EngineProvenance;
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(root, args).unwrap()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

fn dialect_identity(dialect: ForgeDialect) -> (&'static str, &'static str) {
    match dialect {
        ForgeDialect::Github => ("github.com", "L2-L3"),
        ForgeDialect::Gitlab => ("gitlab.com", "L2-3"),
        ForgeDialect::Gitea => ("codeberg.org", "L2-L3"),
    }
}

fn run(
    dialect: ForgeDialect,
    fragment: &str,
    base_target: &str,
    candidate_target: &str,
) -> serde_json::Value {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("README.md"),
        format!("The implementation is [here](src/lib.rs#{fragment}).\n"),
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), base_target).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = Oid::new(
        ObjectFormat::Sha1,
        git(root, &["rev-parse", "HEAD"]).trim().to_owned(),
    )
    .unwrap();

    fs::write(root.join("src/lib.rs"), candidate_target).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = Oid::new(
        ObjectFormat::Sha1,
        git(root, &["rev-parse", "HEAD"]).trim().to_owned(),
    )
    .unwrap();

    let (host, _) = dialect_identity(dialect);
    let context = ForgeContext {
        host: host.to_owned(),
        dialect,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/main".to_owned(),
        default_ref: "refs/heads/main".to_owned(),
        candidate_oid: Some(candidate.as_str().to_owned()),
    };
    let shell = SetupShell {
        engine: engine(),
        enforce: false,
        repository: Some(RepositoryIdentity {
            host: host.to_owned(),
            owner: "acme".to_owned(),
            name: "widgets".to_owned(),
        }),
        forge: Some(dialect),
        candidate_ref: Some("refs/heads/main".to_owned()),
        default_branch_ref: Some("refs/heads/main".to_owned()),
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &shell.engine,
        Some(&context),
        &shell,
        &base,
        &candidate,
    );
    serde_json::from_slice::<serde_json::Value>(&built.wire()).unwrap()["payload"].clone()
}

fn kinds(payload: &serde_json::Value) -> Vec<&str> {
    payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|finding| finding["kind"].as_str())
        .collect()
}

#[test]
fn every_forge_dialect_compares_the_selected_lines() {
    let base = "outside\nselected one\nselected two\ntail\n";
    let candidate = "outside\nselected changed\nselected two\ntail\n";
    for dialect in [
        ForgeDialect::Github,
        ForgeDialect::Gitlab,
        ForgeDialect::Gitea,
    ] {
        let (_, fragment) = dialect_identity(dialect);
        let payload = run(dialect, fragment, base, candidate);
        assert!(
            kinds(&payload).contains(&"dependency-changed-subject-unchanged"),
            "{} must evaluate its own line-range spelling",
            dialect.as_str(),
        );
        assert_eq!(
            payload["observations"][0]["target_change"],
            "changed",
            "{} compares the selected bytes",
            dialect.as_str(),
        );
    }
}

#[test]
fn bytes_outside_the_selection_do_not_create_drift() {
    let payload = run(
        ForgeDialect::Github,
        "L2-L3",
        "outside\nselected one\nselected two\ntail\n",
        "outside changed\nselected one\nselected two\ntail\n",
    );
    assert!(!kinds(&payload).contains(&"dependency-changed-subject-unchanged"));
    assert_eq!(payload["observations"][0]["target_change"], "equal");
    assert_ne!(
        payload["observations"][0]["base"]["resolution"]["target"]["content"]["raw_digest"],
        payload["observations"][0]["candidate"]["resolution"]["target"]["content"]["raw_digest"],
        "the whole blob changed even though the selected projection did not",
    );
}

#[test]
fn a_range_that_leaves_the_blob_is_a_missing_target() {
    let payload = run(
        ForgeDialect::Gitlab,
        "L2-3",
        "one\ntwo\nthree\n",
        "one\ntwo\n",
    );
    assert!(kinds(&payload).contains(&"explicit-target-missing"));
    let resolution = &payload["observations"][0]["candidate"]["resolution"];
    assert_eq!(resolution["kind"], "missing");
    assert_eq!(resolution["reason"], "line-fragment-out-of-range");
    assert_eq!(resolution["path"], "src/lib.rs");
}
