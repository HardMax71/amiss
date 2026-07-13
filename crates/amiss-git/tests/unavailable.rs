#![cfg(not(unix))]

use amiss_git::{Error, Repository};
use amiss_wire::model::ObjectFormat;

#[test]
fn a_platform_without_the_handle_boundary_reports_unavailable() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    std::fs::create_dir_all(dir.path().join(".git/objects")).expect("git layout");
    let opened = Repository::open(dir.path(), ObjectFormat::Sha1);
    assert!(
        matches!(opened, Err(Error::RepositoryUnavailable)),
        "no pathname-traversal fallback exists on this platform"
    );
}
