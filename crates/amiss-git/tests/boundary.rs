use std::fs;
use std::path::Path;

use amiss_fixtures::directory_link;
use amiss_git::{Error, GitLimits, GitResources, Repository};
use amiss_wire::model::{ObjectFormat, Oid};
use tempfile::TempDir;

/// The handle/no-follow boundary is one law with one wording on every
/// platform, so it gets one test file that runs on every platform. Unix
/// refuses the reparse point in the open itself, with `O_NOFOLLOW`. Windows
/// opens the reparse point rather than its target and refuses it by its
/// attribute. The fixture links are symlinks on unix and junctions on
/// Windows, and a junction is what an unprivileged Windows process can
/// actually create, which is what keeps these assertions running on an
/// ordinary CI runner instead of being quietly skipped.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn repository(at: &Path) {
    fs::create_dir_all(at.join(".git/objects")).unwrap();
}

#[test]
fn an_ordinary_repository_opens_through_the_boundary() {
    let dir = TempDir::new().unwrap();
    repository(dir.path());
    assert!(
        Repository::open(dir.path(), ObjectFormat::Sha1).is_ok(),
        "an ordinary root, .git, and objects directory open"
    );
}

#[test]
fn a_reparse_point_at_the_root_is_refused() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    repository(&real);
    let alias = dir.path().join("alias");
    directory_link(&real, &alias).unwrap();
    assert_eq!(
        Repository::open(&alias, ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "the root's final entry is never followed"
    );
}

#[test]
fn a_reparse_point_at_the_git_directory_is_refused() {
    let dir = TempDir::new().unwrap();
    let store = dir.path().join("store");
    fs::create_dir_all(store.join("objects")).unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(&root).unwrap();
    directory_link(&store, &root.join(".git")).unwrap();
    assert_eq!(
        Repository::open(&root, ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "the .git child is never followed"
    );
}

#[test]
fn a_reparse_point_at_the_objects_directory_is_refused() {
    let dir = TempDir::new().unwrap();
    let store = dir.path().join("store");
    fs::create_dir_all(&store).unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join(".git")).unwrap();
    directory_link(&store, &root.join(".git/objects")).unwrap();
    assert_eq!(
        Repository::open(&root, ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "the objects directory is never followed"
    );
}

#[test]
fn a_reparse_point_in_the_object_path_is_unreadable_not_absent() {
    let dir = TempDir::new().unwrap();
    repository(dir.path());
    let store = dir.path().join("store");
    fs::create_dir_all(&store).unwrap();
    directory_link(&store, &dir.path().join(".git/objects/aa")).unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let oid = Oid::new(ObjectFormat::Sha1, format!("aa{}", "b".repeat(38))).unwrap();
    assert_eq!(
        repo.read_object(&mut resources, &oid).unwrap_err(),
        Error::ObjectUnreadable,
        "a refused reparse point is never mistaken for an absent object"
    );
}

#[test]
fn a_git_directory_that_is_a_file_is_refused() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".git"), "gitdir: elsewhere\n").unwrap();
    assert_eq!(
        Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "a linked worktree's .git file is not a directory"
    );
}

#[test]
fn an_absent_repository_is_refused() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        Repository::open(&dir.path().join("nowhere"), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "an absent root"
    );
    assert_eq!(
        Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "an absent .git"
    );
}
