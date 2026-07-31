#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use amiss_bootstrap::constraint::{ConstraintError, derive_execution_constraint};
use amiss_bootstrap::{BOOTSTRAP_DOMAIN, validate};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::controls::ExecutionConstraintDescriptor;
use amiss_wire::digest::hb;
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

mod support;

use amiss_fixtures::executable_bytes as engine_bytes;

use support::release::{Release, release};

fn identity() -> RepositoryIdentity {
    RepositoryIdentity::new(
        "git.example.internal".to_owned(),
        "platform/security".to_owned(),
        "amiss".to_owned(),
    )
    .unwrap()
}

fn derive(
    release: &Release,
    bootstrap: &[u8],
) -> Result<ExecutionConstraintDescriptor, ConstraintError> {
    let repository = Repository::open(release.dir.path(), ObjectFormat::Sha1).unwrap();
    let commit = Oid::new(ObjectFormat::Sha1, release.commit.clone()).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    derive_execution_constraint(
        &repository,
        &mut resources,
        &identity(),
        &commit,
        "amiss / assure",
        bootstrap,
    )
}

#[test]
fn derivation_pins_and_validates_the_exact_release() {
    let release = release(|_root| {});
    let bootstrap = engine_bytes(release.platform);
    let descriptor = derive(&release, &bootstrap).unwrap();
    assert_eq!(descriptor.action_repository, identity());
    assert_eq!(descriptor.action_object_format, ObjectFormat::Sha1);
    assert_eq!(descriptor.action_commit_oid.as_str(), release.commit);
    assert_eq!(descriptor.action_tree_oid.as_str(), release.tree);
    assert_eq!(descriptor.manifest_path.as_str(), "release-manifest.json");
    assert_eq!(descriptor.release_manifest_digest, release.manifest_digest);
    assert_eq!(descriptor.selected_platform, release.platform);
    assert_eq!(descriptor.required_status_name, "amiss / assure");
    assert_eq!(
        descriptor.bootstrap_digest,
        hb(BOOTSTRAP_DOMAIN, &bootstrap)
    );

    let canonical = descriptor.canonical_bytes().unwrap();
    assert_eq!(
        ExecutionConstraintDescriptor::parse(&canonical).unwrap(),
        descriptor
    );
    assert_eq!(
        derive(&release, &bootstrap).unwrap().canonical_bytes(),
        Ok(canonical)
    );

    let repository = Repository::open(release.dir.path(), ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let validated = validate(&repository, &mut resources, &descriptor, &bootstrap).unwrap();
    assert_eq!(validated.engine_digest, release.engine_digest);
}

#[test]
fn derivation_reads_the_commit_not_the_worktree() {
    let release = release(|_root| {});
    let bootstrap = engine_bytes(release.platform);
    let expected = derive(&release, &bootstrap)
        .unwrap()
        .canonical_bytes()
        .unwrap();
    std::fs::write(
        release.dir.path().join("release-manifest.json"),
        b"changed worktree",
    )
    .unwrap();
    std::fs::write(
        release.dir.path().join("dist/launcher.js"),
        b"changed worktree",
    )
    .unwrap();
    let actual = derive(&release, &bootstrap)
        .unwrap()
        .canonical_bytes()
        .unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn derivation_refuses_an_invalid_marker_closure_or_bootstrap() {
    let missing = release(|root| {
        std::fs::remove_file(root.join("release-manifest.digest")).unwrap();
    });
    let bootstrap = engine_bytes(missing.platform);
    assert_eq!(
        derive(&missing, &bootstrap).unwrap_err().reason,
        "path-not-regular-blob"
    );

    let malformed = release(|root| {
        std::fs::write(root.join("release-manifest.digest"), b"not a digest\n").unwrap();
    });
    let bootstrap = engine_bytes(malformed.platform);
    assert_eq!(
        derive(&malformed, &bootstrap).unwrap_err().reason,
        "manifest-digest-mismatch"
    );

    let non_regular = release(|root| {
        let marker = root.join("release-manifest.digest");
        std::fs::remove_file(&marker).unwrap();
        std::fs::create_dir(&marker).unwrap();
        std::fs::write(marker.join("nested"), b"not a marker").unwrap();
    });
    let bootstrap = engine_bytes(non_regular.platform);
    assert_eq!(
        derive(&non_regular, &bootstrap).unwrap_err().reason,
        "path-not-regular-blob"
    );

    let marked = release(|root| {
        std::fs::write(
            root.join("release-manifest.digest"),
            format!("sha256:{}\n", "0".repeat(64)),
        )
        .unwrap();
    });
    let bootstrap = engine_bytes(marked.platform);
    assert_eq!(
        derive(&marked, &bootstrap).unwrap_err().reason,
        "manifest-digest-mismatch"
    );

    let changed = release(|root| {
        std::fs::write(root.join("action.yml"), b"changed before commit").unwrap();
    });
    let bootstrap = engine_bytes(changed.platform);
    assert_eq!(
        derive(&changed, &bootstrap).unwrap_err().reason,
        "runtime-closure-mismatch"
    );

    let release = release(|_root| {});
    assert_eq!(
        derive(&release, b"not an executable").unwrap_err().reason,
        "bootstrap-platform-mismatch"
    );
}
