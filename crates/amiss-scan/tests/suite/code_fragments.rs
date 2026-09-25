#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "integration assertions over a controlled report fixture"
)]

use sha2::Digest as _;
use std::fs;
use std::path::Path;

use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair};
use amiss_scan::report::RequestDigests;
use amiss_scan::resolve::ForgeContext;
use amiss_wire::branch_ref;
use amiss_wire::model::{Digest, ForgeDialect, ObjectFormat, Oid, RepoPath, RepositoryIdentity};
use amiss_wire::report::EngineProvenance;
use amiss_wire::report::model::{TargetChange, occurrences};
use amiss_wire::resolution::{BlobContent, BlobTarget, Missing, Resolution, Target};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(root, args).unwrap()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: Digest::from(
            sha2::Sha256::new_with_prefix("amiss/scanner-engine")
                .chain_update([0_u8])
                .chain_update(b"test engine")
                .finalize()
                .0,
        ),
    }
}

fn dialect_identity(dialect: ForgeDialect) -> (&'static str, &'static str) {
    match dialect {
        ForgeDialect::Github => ("github.com", "L2-L3"),
        ForgeDialect::Gitlab => ("gitlab.com", "L2-3"),
        ForgeDialect::Gitea => ("codeberg.org", "L2-L3"),
        ForgeDialect::BitbucketCloud => ("bitbucket.org", "lib.rs-2"),
        ForgeDialect::BitbucketDataCenter => ("bitbucket.example", "2-3"),
    }
}

fn run(
    dialect: ForgeDialect,
    fragment: &str,
    base_target: &str,
    candidate_target: &str,
) -> (amiss_scan::report::Built, serde_json::Value) {
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
        dialect,
        object_format: ObjectFormat::Sha1,

        repository: RepositoryIdentity::new(
            host.to_owned(),
            "acme".to_owned(),
            "widgets".to_owned(),
        )
        .unwrap(),
        candidate_ref: Some(branch_ref!("refs/heads/main")),
        default_ref: Some(branch_ref!("refs/heads/main")),
        default_aliases: Vec::new(),
    };
    let shell = SetupShell {
        engine: engine(),
        profile: amiss_wire::controls::Profile::Observe,
        repository: Some(
            RepositoryIdentity::new(host.to_owned(), "acme".to_owned(), "widgets".to_owned())
                .unwrap(),
        ),
        forge: Some(dialect),
        candidate_ref: Some(branch_ref!("refs/heads/main")),
        target_ref: None,
        default_branch_ref: Some(branch_ref!("refs/heads/main")),
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: amiss_scan::semantic::Input::None,
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
    )
    .unwrap();
    let payload = crate::support::generated_report(&amiss_scan::report::wire(&built).unwrap())
        .unwrap()["payload"]
        .take();
    (built, payload)
}

/// The raw digest of a resolved or mismatched blob target.
fn raw_digest(resolution: &Resolution<RepoPath>) -> Option<Digest> {
    match resolution {
        Resolution::Resolved {
            target: Target::Blob(BlobTarget { content, .. }),
        }
        | Resolution::TypeMismatch {
            target: Target::Blob(BlobTarget { content, .. }),
        } => Some(match content {
            BlobContent::Available { raw_digest, .. } | BlobContent::LfsPointer { raw_digest } => {
                *raw_digest
            }
        }),
        Resolution::Resolved {
            target: Target::Tree { .. },
        }
        | Resolution::TypeMismatch {
            target: Target::Tree { .. },
        }
        | Resolution::Missing(_)
        | Resolution::DeclaredUntracked(_)
        | Resolution::UnsupportedTarget(_)
        | Resolution::UnsupportedSemantics(_)
        | Resolution::UnsupportedVersion { .. }
        | Resolution::Invalid { .. }
        | Resolution::External { .. } => None,
    }
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
        ForgeDialect::BitbucketCloud,
        ForgeDialect::BitbucketDataCenter,
    ] {
        let (_, fragment) = dialect_identity(dialect);
        let (_built, payload) = run(dialect, fragment, base, candidate);
        assert!(
            kinds(&payload).contains(&"dependency-changed-subject-unchanged"),
            "{} must evaluate its own line-range spelling",
            dialect.as_ref(),
        );
        assert_eq!(
            payload["observations"][0]["target_change"],
            "changed",
            "{} compares the selected bytes",
            dialect.as_ref(),
        );
    }
}

#[test]
fn bytes_outside_the_selection_do_not_create_drift() {
    let (built, payload) = run(
        ForgeDialect::Github,
        "L2-L3",
        "outside\nselected one\nselected two\ntail\n",
        "outside changed\nselected one\nselected two\ntail\n",
    );
    assert!(!kinds(&payload).contains(&"dependency-changed-subject-unchanged"));
    let row = built.envelope.payload.observations.first().unwrap();
    assert_eq!(row.target_change, TargetChange::Equal);
    assert_ne!(
        occurrences(row)
            .base
            .map(|base| raw_digest(&base.resolution)),
        occurrences(row)
            .candidate
            .map(|side| raw_digest(&side.resolution)),
        "the whole blob changed even though the selected projection did not",
    );
}

#[test]
fn a_range_that_leaves_the_blob_is_a_missing_target() {
    let (built, payload) = run(
        ForgeDialect::Gitlab,
        "L2-3",
        "one\ntwo\nthree\n",
        "one\ntwo\n",
    );
    assert!(kinds(&payload).contains(&"explicit-target-missing"));
    let candidate = occurrences(built.envelope.payload.observations.first().unwrap())
        .candidate
        .unwrap();
    let Resolution::Missing(Missing::LineFragmentOutOfRange { path }) = &candidate.resolution
    else {
        panic!("the range leaves the blob: {:?}", candidate.resolution);
    };
    assert_eq!(path.as_str(), Some("src/lib.rs"));
}
