#![cfg(test)]

use std::path::Path;

use super::{PathRequirements, canonical_path, resolve_execution_paths, separate_roots};

fn directory() -> PathRequirements {
    PathRequirements {
        accepts: std::fs::FileType::is_dir,
        invalid: "not a directory",
        unresolved: "unresolved",
    }
}

/// A configured path is absolute, real, and of the kind it was asked for.
#[test]
fn a_configured_path_is_absolute_real_and_of_its_kind() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let file = root.path().join("occupant");
    std::fs::write(&file, b"here").expect("write");

    assert!(canonical_path(root.path(), directory()).is_ok());
    assert_eq!(
        canonical_path(&file, directory())
            .expect_err("a file where a directory belongs")
            .to_string(),
        "not a directory"
    );
    assert_eq!(
        canonical_path(Path::new("scratch"), directory())
            .expect_err("a relative path names nothing certain")
            .to_string(),
        "not a directory"
    );
    assert_eq!(
        canonical_path(&root.path().join("absent"), directory())
            .expect_err("a path nothing stands at")
            .to_string(),
        "not a directory"
    );
}

/// Two roots are separate when neither contains the other, in either order.
#[test]
fn roots_are_separate_in_both_directions() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let outer = root.path();
    let inner = outer.join("inside");
    std::fs::create_dir(&inner).expect("create");
    let sibling = outer.join("beside");
    std::fs::create_dir(&sibling).expect("create");
    let overlap = "roots overlap";

    assert!(separate_roots([&inner, &sibling], overlap).is_ok());
    assert_eq!(
        separate_roots([outer, &inner], overlap)
            .expect_err("the first contains the second")
            .to_string(),
        overlap
    );
    assert_eq!(
        separate_roots([&inner, outer], overlap)
            .expect_err("the second contains the first")
            .to_string(),
        overlap
    );
    assert_eq!(
        separate_roots([&inner, &inner], overlap)
            .expect_err("a root against itself")
            .to_string(),
        overlap
    );
}

fn plan_over(constraint: &[u8]) -> amiss_controller::CheckPlan {
    use amiss_controller::{PolicyControls, check_plan};
    use amiss_wire::controls::{Profile, parse_execution_constraint};

    let descriptor = parse_execution_constraint(constraint).expect("a constraint");
    check_plan(Profile::Enforce, PolicyControls::default(), descriptor).expect("a check plan")
}

/// The bootstrap on disk must be the one the constraint names, and the
/// constraint must target this host.
#[test]
fn execution_paths_bind_the_bootstrap_and_the_host() {
    use amiss_wire::action::host_platform;

    let trust =
        amiss_controller_fixtures::config::TrustFiles::new("forge.example", "acme", "widget")
            .expect("trust files");
    let scratch = trust.directory("scratch").expect("scratch");
    let ledger = trust.directory("ledger").expect("ledger");
    let artifacts = trust.directory("artifacts").expect("artifacts");
    let bytes = std::fs::read(&trust.constraint).expect("constraint bytes");
    let plan = plan_over(&bytes);

    assert!(
        resolve_execution_paths(&trust.bootstrap, &scratch, &ledger, &artifacts, &plan).is_ok(),
        "the bootstrap the constraint names, on the host it targets"
    );

    let other = trust.path("other-bootstrap");
    std::fs::write(&other, b"another bootstrap entirely").expect("write");
    assert_eq!(
        resolve_execution_paths(&other, &scratch, &ledger, &artifacts, &plan)
            .err()
            .expect("another bootstrap under the same constraint")
            .to_string(),
        "bootstrap does not match the execution constraint"
    );

    let here = host_platform().expect("a platform for this host");
    let there = ["linux-x86_64", "macos-aarch64"]
        .into_iter()
        .find(|name| *name != here.as_ref())
        .expect("a platform this host is not");
    let elsewhere = String::from_utf8(bytes)
        .expect("constraint text")
        .replace(here.as_ref(), there);
    assert_eq!(
        resolve_execution_paths(
            &trust.bootstrap,
            &scratch,
            &ledger,
            &artifacts,
            &plan_over(elsewhere.as_bytes())
        )
        .err()
        .expect("the same bootstrap under a constraint aimed elsewhere")
        .to_string(),
        "execution constraint does not target this host"
    );
}
