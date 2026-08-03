#![cfg(test)]

use std::path::Path;

use super::{
    ConfigError, PathRequirements, canonical_path, resolve_execution_paths, separate_roots,
};

fn directory() -> PathRequirements {
    PathRequirements {
        accepts: std::fs::FileType::is_dir,
        invalid: ConfigError("not a directory"),
        unresolved: ConfigError("unresolved"),
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
        canonical_path(&file, directory()),
        Err(ConfigError("not a directory")),
        "a file where a directory belongs"
    );
    assert_eq!(
        canonical_path(Path::new("scratch"), directory()),
        Err(ConfigError("not a directory")),
        "a relative path names nothing certain"
    );
    assert_eq!(
        canonical_path(&root.path().join("absent"), directory()),
        Err(ConfigError("not a directory")),
        "a path nothing stands at"
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
    let overlap = ConfigError("roots overlap");

    assert_eq!(separate_roots([&inner, &sibling], overlap), Ok(()));
    assert_eq!(
        separate_roots([outer, &inner], overlap),
        Err(overlap),
        "the first contains the second"
    );
    assert_eq!(
        separate_roots([&inner, outer], overlap),
        Err(overlap),
        "the second contains the first"
    );
    assert_eq!(
        separate_roots([&inner, &inner], overlap),
        Err(overlap),
        "a root against itself"
    );
}

/// The bootstrap on disk must be the one the constraint names, and the
/// constraint must target this host.
#[test]
fn execution_paths_bind_the_bootstrap_and_the_host() {
    use amiss_controller::{PolicyControls, check_plan};
    use amiss_wire::controls::{ExecutionConstraintDescriptor, Profile};

    let trust =
        amiss_controller_fixtures::config::TrustFiles::new("forge.example", "acme", "widget")
            .expect("trust files");
    let scratch = trust.directory("scratch").expect("scratch");
    let ledger = trust.directory("ledger").expect("ledger");
    let constraint = ExecutionConstraintDescriptor::parse(
        &std::fs::read(&trust.constraint).expect("constraint bytes"),
    )
    .expect("a constraint");
    let plan =
        check_plan(Profile::Enforce, PolicyControls::default(), constraint).expect("a check plan");

    assert!(
        resolve_execution_paths(&trust.bootstrap, &scratch, &ledger, &plan).is_ok(),
        "the bootstrap the constraint names, on the host it targets"
    );

    let other = trust.path("other-bootstrap");
    std::fs::write(&other, b"another bootstrap entirely").expect("write");
    assert_eq!(
        resolve_execution_paths(&other, &scratch, &ledger, &plan).err(),
        Some(ConfigError(
            "bootstrap does not match the execution constraint"
        )),
        "another bootstrap under the same constraint"
    );
}
